/// Chat export module.
/// Decrypts SQLCipher database and exports messages as TXT or CSV.

use crate::analysis::{normalize_ts, ts_to_str};
use crate::message_parser::extract_text;
use chrono::Datelike;
use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct ExportResult {
    pub total: usize,
    pub groups: usize,
    pub private: usize,
    pub files: Vec<String>,
}

/// Open an encrypted QQ database. Uses a persistent disk cache so the DB
/// is only copied once per session (subsequent calls return immediately).
pub fn open_db_for_analysis(db_path: &str, key: &str) -> Result<Connection, String> {
    let cache_path = get_cached_db(db_path)?;
    open_db_at_path(&cache_path, key)
}

/// Returns path to cached (header-stripped) DB, creating it if needed.
fn get_cached_db(db_path: &str) -> Result<std::path::PathBuf, String> {
    use std::hash::{Hash, Hasher};
    use std::io::{Seek, SeekFrom};
    use std::time::Instant;

    let meta = fs::metadata(db_path).map_err(|e| format!("读取数据库失败: {}", e))?;
    if meta.len() <= 1024 {
        return Err("数据库文件过小".to_string());
    }

    // Cache key = hash of path + file size + modification time
    let mut h = std::collections::hash_map::DefaultHasher::new();
    db_path.hash(&mut h);
    meta.len().hash(&mut h);
    meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH).hash(&mut h);
    let cache_key = format!("{:x}", h.finish());

    let cache_dir = std::env::temp_dir().join("qqflow_cache");
    fs::create_dir_all(&cache_dir).ok();
    let cache_path = cache_dir.join(&cache_key);

    // Only copy if cache doesn't exist
    if !cache_path.exists() {
        let t0 = Instant::now();
        let db_size_mb = meta.len() as f64 / 1_048_576.0;
        let mut src = fs::File::open(db_path)
            .map_err(|e| format!("读取数据库失败: {}", e))?;
        src.seek(SeekFrom::Start(1024))
            .map_err(|e| format!("跳过文件头失败: {}", e))?;
        let mut dst = fs::File::create(&cache_path)
            .map_err(|e| format!("创建缓存文件失败: {}", e))?;
        std::io::copy(&mut src, &mut dst)
            .map_err(|e| format!("复制数据库失败: {}", e))?;
        // Write timing info to a known location so we can debug even without progress log
        let _ = std::fs::write(
            cache_dir.join("last_copy_time.txt"),
            format!("{:.1} MB in {:.1}s\n", db_size_mb, t0.elapsed().as_secs_f32()),
        );
    }

    Ok(cache_path)
}

/// Open a cached DB file with SQLCipher.
fn open_db_at_path(path: &std::path::Path, key: &str) -> Result<Connection, String> {
    let conn = Connection::open(path).map_err(|e| format!("打开数据库失败: {}", e))?;

    // Configure SQLCipher
    conn.execute_batch(&format!("PRAGMA key = '{}'", key.replace('\'', "''")))
        .map_err(|e| format!("设置密钥失败: {}", e))?;
    conn.execute_batch("PRAGMA cipher_page_size = 4096")
        .map_err(|e| format!("设置页大小失败: {}", e))?;
    conn.execute_batch("PRAGMA kdf_iter = 4000")
        .map_err(|e| format!("设置KDF迭代失败: {}", e))?;
    conn.execute_batch("PRAGMA cipher_hmac_algorithm = HMAC_SHA1")
        .map_err(|e| format!("设置HMAC失败: {}", e))?;
    conn.execute_batch("PRAGMA cipher_default_kdf_algorithm = PBKDF2_HMAC_SHA512")
        .map_err(|e| format!("设置KDF算法失败: {}", e))?;
    conn.execute_batch("PRAGMA cipher = 'aes-256-cbc'")
        .map_err(|e| format!("设置加密算法失败: {}", e))?;

    // Verify decryption
    let result: Result<i64, _> =
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |row| row.get(0));

    if result.is_err() {
        // Try HMAC_SHA512
        conn.execute_batch("PRAGMA cipher_hmac_algorithm = HMAC_SHA512")
            .map_err(|e| format!("设置HMAC_SHA512失败: {}", e))?;
        let result2: i64 = conn
            .query_row("SELECT count(*) FROM sqlite_master", [], |row| row.get(0))
            .map_err(|_| "数据库解密失败，请检查密钥是否正确".to_string())?;
        let _ = result2;
    }

    Ok(conn)
}

// ── In-memory message store ──
// Replaces SQL queries with streaming full-table scan + HashMap grouping.
// One 1-2 second scan replaces 40-second index creation + per-group queries.

#[derive(Debug, Clone)]
pub struct GroupMsg {
    pub msg_id: i64,
    pub uid: String,
    pub nick: String,
    pub blob: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct C2cMsg {
    pub msg_id: i64,
    pub peer: String,
    pub nick: String,
    pub blob: Vec<u8>,
}

pub struct MessageStore {
    pub group_msgs: HashMap<String, Vec<GroupMsg>>,
    pub c2c_msgs: HashMap<String, Vec<C2cMsg>>,
    pub uid_map: HashMap<String, String>,
    pub group_names: HashMap<String, String>,
}

impl MessageStore {
    /// Stream all messages from the database and build in-memory HashMaps.
    /// No writes to disk — purely read-only streaming scan.
    pub fn load(conn: &Connection) -> Result<Self, String> {
        use std::time::Instant;

        eprintln!("[MessageStore::load] 开始加载 UID 映射和群名称...");
        let t0 = Instant::now();
        let uid_map = load_uid_map(conn);
        let group_names = load_group_names(conn);
        eprintln!("[MessageStore::load] UID={}, 群名={}, 耗时 {:.1}s",
            uid_map.len(), group_names.len(), t0.elapsed().as_secs_f32());

        // ── Group messages ──
        let mut group_msgs: HashMap<String, Vec<GroupMsg>> = HashMap::new();
        if let Ok(mut stmt) = conn.prepare(
            "SELECT \"40021\", \"40001\", \"40020\", \"40093\", \"40800\" FROM group_msg_table"
        ) {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0).unwrap_or_default(),
                    row.get::<_, i64>(1).unwrap_or(0),
                    row.get::<_, String>(2).unwrap_or_default(),
                    row.get::<_, String>(3).unwrap_or_default(),
                    row.get::<_, Vec<u8>>(4).unwrap_or_default(),
                ))
            }) {
                eprintln!("[MessageStore::load] 开始加载群消息...");
                let mut count = 0u64;
                let t1 = Instant::now();
                for row in rows.flatten() {
                    let (gid, msg_id, uid, nick, blob) = row;
                    group_msgs.entry(gid).or_default().push(GroupMsg { msg_id, uid, nick, blob });
                    count += 1;
                    if count % 50000 == 0 {
                        eprintln!("[MessageStore::load] 群消息进度: {} 条, {:.1}s", count, t1.elapsed().as_secs_f32());
                    }
                }
                eprintln!("[MessageStore::load] 群消息加载完成: {} 条, {} 个群, {:.1}s",
                    count, group_msgs.len(), t1.elapsed().as_secs_f32());
            }
        }

        // ── C2C messages ──
        let mut c2c_msgs: HashMap<String, Vec<C2cMsg>> = HashMap::new();
        if let Ok(mut stmt) = conn.prepare(
            "SELECT \"40020\", \"40001\", \"40093\", \"40800\" FROM c2c_msg_table"
        ) {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0).unwrap_or_default(),
                    row.get::<_, i64>(1).unwrap_or(0),
                    row.get::<_, String>(2).unwrap_or_default(),
                    row.get::<_, Vec<u8>>(3).unwrap_or_default(),
                ))
            }) {
                eprintln!("[MessageStore::load] 开始加载私聊消息...");
                let mut count = 0u64;
                for row in rows.flatten() {
                    let (peer, msg_id, nick, blob) = row;
                    let peer_clone = peer.clone();
                    c2c_msgs.entry(peer).or_default().push(C2cMsg { msg_id, peer: peer_clone, nick, blob });
                    count += 1;
                }
                eprintln!("[MessageStore::load] 私聊消息加载完成: {} 条, {} 个联系人", count, c2c_msgs.len());
            }
        }

        Ok(MessageStore { group_msgs, c2c_msgs, uid_map, group_names })
    }

    /// Load the store from a database file, with progress feedback via optional log.
    pub fn load_with_progress(
        db_path: &str, key: &str,
        progress_log: Option<&std::sync::Mutex<Vec<crate::commands::LogMessage>>>,
    ) -> Result<Self, String> {
        use std::time::Instant;

        let log = |text: &str| {
            eprintln!("[MessageStore] {}", text);
            if let Some(log) = progress_log {
                if let Ok(mut l) = log.lock() {
                    l.push(crate::commands::LogMessage { level: "info".to_string(), text: text.to_string() });
                }
            }
        };

        log("正在打开数据库...");
        let conn = open_db_for_analysis(db_path, key)?;

        log("正在加载UID映射和群名称...");
        let t0 = Instant::now();
        let uid_map = load_uid_map(&conn);
        let group_names = load_group_names(&conn);
        log(&format!("UID映射 {} 条, 群名称 {} 个 ({:.1}s)", uid_map.len(), group_names.len(), t0.elapsed().as_secs_f32()));

        // ── Group messages ──
        log("正在加载群消息...");
        let mut group_msgs: HashMap<String, Vec<GroupMsg>> = HashMap::new();
        if let Ok(mut stmt) = conn.prepare(
            "SELECT \"40021\", \"40001\", \"40020\", \"40093\", \"40800\" FROM group_msg_table"
        ) {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0).unwrap_or_default(),
                    row.get::<_, i64>(1).unwrap_or(0),
                    row.get::<_, String>(2).unwrap_or_default(),
                    row.get::<_, String>(3).unwrap_or_default(),
                    row.get::<_, Vec<u8>>(4).unwrap_or_default(),
                ))
            }) {
                let mut count = 0u64;
                let t1 = Instant::now();
                for row in rows.flatten() {
                    if count > 0 && count % 50000 == 0 {
                        log(&format!("群消息进度: {} 条 ({:.1}s)...", count, t1.elapsed().as_secs_f32()));
                    }
                    let (gid, msg_id, uid, nick, blob) = row;
                    group_msgs.entry(gid).or_default().push(GroupMsg { msg_id, uid, nick, blob });
                    count += 1;
                }
                log(&format!("群消息加载完成: {} 条, {} 个群 ({:.1}s)", count, group_msgs.len(), t1.elapsed().as_secs_f32()));
            }
        }

        // ── C2C messages ──
        log("正在加载私聊消息...");
        let mut c2c_msgs: HashMap<String, Vec<C2cMsg>> = HashMap::new();
        let t2 = Instant::now();
        if let Ok(mut stmt) = conn.prepare(
            "SELECT \"40020\", \"40001\", \"40093\", \"40800\" FROM c2c_msg_table"
        ) {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0).unwrap_or_default(),
                    row.get::<_, i64>(1).unwrap_or(0),
                    row.get::<_, String>(2).unwrap_or_default(),
                    row.get::<_, Vec<u8>>(3).unwrap_or_default(),
                ))
            }) {
                let mut count = 0u64;
                for row in rows.flatten() {
                    let (peer, msg_id, nick, blob) = row;
                    let peer_clone = peer.clone();
                    c2c_msgs.entry(peer).or_default().push(C2cMsg { msg_id, peer: peer_clone, nick, blob });
                    count += 1;
                }
                log(&format!("私聊消息加载完成: {} 条, {} 个联系人 ({:.1}s)", count, c2c_msgs.len(), t2.elapsed().as_secs_f32()));
            }
        }

        Ok(MessageStore { group_msgs, c2c_msgs, uid_map, group_names })
    }
}

/// Discover table columns and return (table_name, column_names).
pub fn list_tables(conn: &Connection) -> Vec<(String, Vec<String>)> {
    let mut tables = Vec::new();
    let Ok(mut stmt) = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'"
    ) else { return tables };

    let names: Vec<String> = stmt.query_map([], |r| r.get(0)).unwrap().flatten().collect();
    for name in names {
        let cols = conn
            .prepare(&format!("PRAGMA table_info(\"{}\")", name.replace('"', "\"\"")))
            .map(|mut s| {
                s.query_map([], |r| r.get::<_, String>(1))
                    .unwrap()
                    .flatten()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        tables.push((name, cols));
    }
    tables
}

/// Load UID to QQ number mapping table.
pub fn load_uid_map(conn: &Connection) -> HashMap<String, String> {
    let mut map = HashMap::new();

    // 策略1: 尝试已知列名 (48901=UID, 40020=QQ号)
    for table in &["nt_uid_mapping_table", "uid_mapping", "buddy_mapping"] {
        if try_load_uid_map_direct(conn, table, "48901", "40020", &mut map) {
            eprintln!("[load_uid_map] 从 {}/48901+40020 加载 {} 条", table, map.len());
            return map;
        }
    }

    // 策略2: 扫描候选表，自动检测 UID/QQ 列
    let candidates = [
        "nt_uid_mapping_table", "uid_mapping", "buddy_mapping",
        "contact", "friends", "buddy_list", "Friends", "buddys",
        "BuddyInfo", "UinPair_Generic", "mr_friend_MicroMsg", "Friends_Groups",
    ];
    for table in &candidates {
        if try_load_uid_map_auto(conn, table, &mut map) {
            eprintln!("[load_uid_map] 从 {} 自动检测加载 {} 条", table, map.len());
            return map;
        }
    }

    // 策略3: 模糊搜索所有表名含 uid/mapping/friend/contact/buddy 的表
    if let Ok(mut stmt) = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND \
         (name LIKE '%uid%' OR name LIKE '%mapping%' OR name LIKE '%friend%' OR name LIKE '%contact%' OR name LIKE '%buddy%')"
    ) {
        if let Ok(names) = stmt.query_map([], |r| r.get::<_, String>(0)) {
            for name in names.flatten() {
                if try_load_uid_map_auto(conn, &name, &mut map) {
                    eprintln!("[load_uid_map] 从 {} 模糊搜索加载 {} 条", name, map.len());
                    return map;
                }
            }
        }
    }

    // 策略4: 暴力扫描所有表，找含 u_ 前缀列的表
    if let Ok(mut stmt) = conn.prepare("SELECT name FROM sqlite_master WHERE type='table'") {
        if let Ok(names) = stmt.query_map([], |r| r.get::<_, String>(0)) {
            for name in names.flatten() {
                if name.contains("msg") || name.contains("sqlite") { continue; }
                if try_load_uid_map_auto(conn, &name, &mut map) {
                    eprintln!("[load_uid_map] 从 {} 全表扫描加载 {} 条", name, map.len());
                    return map;
                }
            }
        }
    }

    eprintln!("[load_uid_map] 所有策略均未找到映射数据");
    map
}

/// 用已知列名直接查询
fn try_load_uid_map_direct(conn: &Connection, table: &str, uid_col: &str, qq_col: &str, map: &mut HashMap<String, String>) -> bool {
    let exists: bool = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
        [table],
        |r| r.get::<_, i64>(0),
    ).unwrap_or(0) > 0;
    if !exists { return false; }

    let sql = format!("SELECT \"{}\", \"{}\" FROM \"{}\" LIMIT 50000", uid_col, qq_col, table);
    let Ok(mut stmt) = conn.prepare(&sql) else { return false; };

    let Ok(rows) = stmt.query_map([], |row| {
        let uid: String = row.get::<_, String>(0)
            .ok().filter(|s| !s.is_empty())
            .unwrap_or_else(|| row.get::<_, i64>(0).map(|v| v.to_string()).unwrap_or_default());
        let qq: String = row.get::<_, String>(1)
            .ok().filter(|s| !s.is_empty())
            .unwrap_or_else(|| row.get::<_, i64>(1).map(|v| v.to_string()).unwrap_or_default());
        Ok((uid, qq))
    }) else { return false; };

    let mut count = 0;
    for row in rows.flatten() {
        if !row.0.is_empty() && !row.1.is_empty() && row.0 != row.1 {
            map.insert(row.0, row.1);
            count += 1;
        }
    }
    count > 0
}

/// 自动检测表中的 UID 列和 QQ 列
fn try_load_uid_map_auto(conn: &Connection, table: &str, map: &mut HashMap<String, String>) -> bool {
    // 检查表存在且有数据
    let row_count: i64 = conn.query_row(
        &format!("SELECT count(*) FROM \"{}\"", table.replace('"', "\"\"")),
        [], |r| r.get(0)
    ).unwrap_or(0);
    if row_count < 2 || row_count > 200000 { return false; }

    // 获取列信息
    let mut col_stmt = match conn.prepare(
        &format!("PRAGMA table_info(\"{}\")", table.replace('"', "\"\""))
    ) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let columns: Vec<(String, String)> = match col_stmt.query_map([], |row| {
        Ok((row.get::<_, String>(1).unwrap_or_default(), row.get::<_, String>(2).unwrap_or_default()))
    }) {
        Ok(rows) => rows.flatten().collect(),
        Err(_) => return false,
    };
    if columns.len() < 2 { return false; }

    // 读取前几行样本数据
    let Ok(mut sample_stmt) = conn.prepare(
        &format!("SELECT * FROM \"{}\" LIMIT 5", table.replace('"', "\"\""))
    ) else { return false; };

    let samples: Vec<Vec<String>> = match sample_stmt.query_map([], |row| {
        let count = row.as_ref().column_count();
        let mut vals = Vec::with_capacity(count);
        for i in 0..count {
            vals.push(
                row.get::<_, String>(i).ok()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| row.get::<_, i64>(i).map(|v| v.to_string()).unwrap_or_default())
            );
        }
        Ok(vals)
    }) {
        Ok(rows) => rows.flatten().collect(),
        Err(_) => return false,
    };
    if samples.is_empty() { return false; }

    // 检测 UID 列: 列名含 uid/uin，或样本值以 u_ 开头
    let mut uid_col: Option<usize> = None;
    // 检测 QQ 列: 列名含 uin/qq/number，或样本值是 5-12 位纯数字
    let mut qq_col: Option<usize> = None;

    for (i, (col_name, col_type)) in columns.iter().enumerate() {
        let lower = col_name.to_lowercase();
        let type_lower = col_type.to_lowercase();

        // 识别 UID 列
        if uid_col.is_none() {
            if lower.contains("uid") || lower == "uin" {
                uid_col = Some(i);
            } else {
                // 检查样本值
                let has_uid_vals = samples.iter().any(|row| {
                    i < row.len() && (row[i].starts_with("u_") || (row[i].len() > 10 && !row[i].chars().all(|c| c.is_ascii_digit())))
                });
                if has_uid_vals && !type_lower.contains("int") {
                    uid_col = Some(i);
                }
            }
        }

        // 识别 QQ 列
        if qq_col.is_none() && uid_col != Some(i) {
            if lower.contains("qq") || lower.contains("uin") || lower.contains("number") {
                qq_col = Some(i);
            } else if type_lower.contains("int") {
                // INTEGER 类型列，检查样本值是否像 QQ 号
                let has_qq_vals = samples.iter().any(|row| {
                    i < row.len() && row[i].len() >= 5 && row[i].len() <= 12 && row[i].chars().all(|c| c.is_ascii_digit())
                });
                if has_qq_vals {
                    qq_col = Some(i);
                }
            } else {
                // TEXT 类型列，检查样本值是否是纯数字 QQ 号
                let has_qq_vals = samples.iter().all(|row| {
                    if i >= row.len() || row[i].is_empty() { return true; }
                    row[i].len() >= 5 && row[i].len() <= 12 && row[i].chars().all(|c| c.is_ascii_digit())
                });
                let has_some_qq = samples.iter().any(|row| {
                    i < row.len() && !row[i].is_empty() && row[i].len() >= 5 && row[i].len() <= 12 && row[i].chars().all(|c| c.is_ascii_digit())
                });
                if has_qq_vals && has_some_qq {
                    qq_col = Some(i);
                }
            }
        }
    }

    let (uid_idx, qq_idx) = match (uid_col, qq_col) {
        (Some(u), Some(q)) => (u, q),
        _ => return false,
    };

    eprintln!("[load_uid_map] auto: table={}, uid_col={}:{}, qq_col={}:{}, rows={}",
        table, uid_idx, columns[uid_idx].0, qq_idx, columns[qq_idx].0, row_count);

    // 加载全部映射
    let Ok(mut full_stmt) = conn.prepare(
        &format!("SELECT * FROM \"{}\" LIMIT 50000", table.replace('"', "\"\""))
    ) else { return false; };

    if let Ok(rows) = full_stmt.query_map([], |row| {
        let count = row.as_ref().column_count();
        let get_str = |idx: usize| -> String {
            if idx >= count { return String::new(); }
            row.get::<_, String>(idx).ok().filter(|s| !s.is_empty())
                .unwrap_or_else(|| row.get::<_, i64>(idx).map(|v| v.to_string()).unwrap_or_default())
        };
        Ok((get_str(uid_idx), get_str(qq_idx)))
    }) {
        for row in rows.flatten() {
            if !row.0.is_empty() && !row.1.is_empty() && row.0 != row.1 {
                map.insert(row.0, row.1);
            }
        }
    }

    !map.is_empty()
}


/// Load group ID to group name mapping.
pub fn load_group_names(conn: &Connection) -> HashMap<String, String> {
    let mut map = HashMap::new();

    // Find a candidate group info table
    let table_name = match find_group_info_table(conn) {
        Some(t) => t,
        None => return map,
    };

    // Get column info
    let columns: Vec<(String, String)> = {
        let Ok(mut stmt) = conn.prepare(
            &format!("PRAGMA table_info(\"{}\")", table_name.replace('"', "\"\""))
        ) else { return map };
        stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(1).unwrap_or_default(),
                row.get::<_, String>(2).unwrap_or_default(),
            ))
        })
        .unwrap()
        .flatten()
        .collect()
    };

    // Find group_id column (type contains "INT" or name contains "uin"/"id"/"code")
    let mut gid_col: Option<usize> = None;
    let mut name_col: Option<usize> = None;

    for (i, (col_name, col_type)) in columns.iter().enumerate() {
        let lower = col_name.to_lowercase();
        let type_lower = col_type.to_lowercase();
        if gid_col.is_none()
            && (type_lower.contains("int")
                || lower.contains("uin")
                || lower.contains("id")
                || lower.contains("code")
                || lower.contains("group"))
        {
            gid_col = Some(i);
        }
        if name_col.is_none()
            && (lower.contains("name")
                || lower.contains("title")
                || lower.contains("remark")
                || type_lower.contains("text")
                || type_lower.contains("varchar"))
        {
            if gid_col != Some(i) {
                name_col = Some(i);
            }
        }
    }

    let (gid_idx, name_idx) = match (gid_col, name_col) {
        (Some(g), Some(n)) => (g, n),
        _ => return map,
    };

    // Load all rows (limit to avoid scanning huge unrelated tables)
    let Ok(mut stmt) = conn.prepare(
        &format!("SELECT * FROM \"{}\" LIMIT 50000", table_name.replace('"', "\"\""))
    ) else { return map };

    if let Ok(rows) = stmt.query_map([], |row| {
        let get_cell = |idx: usize| -> String {
            row.get::<_, String>(idx)
                .ok()
                .filter(|s: &String| !s.is_empty())
                .unwrap_or_else(|| {
                    row.get::<_, i64>(idx).map(|v| v.to_string()).unwrap_or_default()
                })
        };
        Ok((get_cell(gid_idx), get_cell(name_idx)))
    }) {
        for row in rows.flatten() {
            if !row.0.is_empty() && !row.1.is_empty() {
                map.insert(row.0, row.1);
            }
        }
    }

    map
}

fn find_group_info_table(conn: &Connection) -> Option<String> {
    let candidates = [
        "nt_group_info",
        "group_info",
        "troop_info",
        "nt_troop_info",
        "nt_group_table",
        "troop_member_list",
        "group_member_list",
        "recent_contact_table",
        "nt_recent_contact_table",
        "aio_recent_contact_table",
        "contact_table",
        "nt_buddylist",
    ];
    for name in &candidates {
        let count: i64 = conn
            .query_row(
                &format!("SELECT count(*) FROM \"{}\"", name),
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if count > 0 {
            return Some(name.to_string());
        }
    }
    // Fallback: find any table with "group", "troop", "recent", "contact", "buddy" in the name
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND (name LIKE '%group%' OR name LIKE '%troop%' OR name LIKE '%recent%' OR name LIKE '%contact%' OR name LIKE '%buddy%') AND name NOT IN ('group_msg_table', 'c2c_msg_table')"
    ).ok()?;
    let names: Vec<String> = stmt.query_map([], |r| r.get::<_, String>(0))
        .ok()?
        .flatten()
        .collect();
    for name in names {
        let count: i64 = conn
            .query_row(
                &format!("SELECT count(*) FROM \"{}\"", name.replace('"', "\"\"")),
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if count > 0 {
            return Some(name);
        }
    }
    None
}

/// Sanitize a string for use as a filename (remove Windows-illegal chars).
pub fn sanitize_filename(s: &str) -> String {
    s.chars().map(|c| match c { '<'|'>'|':'|'"'|'/'|'\\'|'|'|'?'|'*' => '_', _ => c }).collect()
}

/// Export all chat records as TXT files.
pub fn decrypt_and_export(
    db_path: &str,
    key: &str,
    output_dir: &str,
    group_ids_filter: Option<&[String]>,
    peer_ids_filter: Option<&[String]>,
    store: Option<&MessageStore>,
    progress_log: Option<&std::sync::Mutex<Vec<crate::commands::LogMessage>>>,
    cancel_flag: Option<&std::sync::atomic::AtomicBool>,
) -> Result<ExportResult, String> {
    use std::sync::atomic::Ordering;

    let is_cancelled = || -> bool {
        cancel_flag.map_or(false, |f| f.load(Ordering::SeqCst))
    };

    // ── Timestamp helpers for readable export ──
    fn format_date(ts: i64) -> String {
        let secs = normalize_ts(ts);
        if secs == 0 { return String::new() }
        chrono::NaiveDateTime::from_timestamp_opt(secs, 0)
            .map(|dt| {
                let weekdays = ["日","一","二","三","四","五","六"];
                let w = weekdays[dt.weekday().num_days_from_sunday() as usize];
                format!("{} 星期{}", dt.format("%Y-%m-%d"), w)
            })
            .unwrap_or_default()
    }
    fn format_time(ts: i64) -> String {
        let secs = normalize_ts(ts);
        if secs == 0 { return String::new() }
        chrono::NaiveDateTime::from_timestamp_opt(secs, 0)
            .map(|dt| dt.format("%H:%M:%S").to_string())
            .unwrap_or_default()
    }
    fs::create_dir_all(output_dir).map_err(|e| format!("创建输出目录失败: {}", e))?;

    let log = |level: &str, text: String| {
        if let Some(log) = progress_log {
            if let Ok(mut l) = log.lock() {
                l.push(crate::commands::LogMessage { level: level.to_string(), text });
            }
        }
    };

    // ── Fast path: in-memory store ──
    if let Some(store) = store {
        let groups_dir = Path::new(output_dir).join("群聊");
        let private_dir = Path::new(output_dir).join("私聊");
        fs::create_dir_all(&groups_dir).ok();
        fs::create_dir_all(&private_dir).ok();
        let mut total = 0usize;
        let mut group_count = 0usize;
        let mut private_count = 0usize;
        let mut files = Vec::new();

        let filter_set: Option<std::collections::HashSet<&str>> = group_ids_filter
            .map(|f| f.iter().map(|s| s.as_str()).collect());
        // 无筛选条件时不导出（空选 = 不导出）
        let group_ids: Vec<&String> = match &filter_set {
            None => Vec::new(),
            Some(f) => store.group_msgs.keys()
                .filter(|gid| f.contains(gid.as_str()))
                .collect(),
        };

        for (idx, gid) in group_ids.iter().enumerate() {
            if is_cancelled() {
                log("warn", "导出已被取消".to_string());
                return Ok(ExportResult { total, groups: group_count, private: private_count, files });
            }
            let msgs = &store.group_msgs[*gid];
            let group_name = store.group_names.get(*gid).map(|s| s.as_str()).unwrap_or("");
            let display = if group_name.is_empty() { format!("群 {}", gid) } else { format!("{} ({})", group_name, gid) };
            log("info", format!("正在导出群聊 ({}/{}) {} - {} 条消息...", idx + 1, group_ids.len(), display, msgs.len()));

            // Sort by timestamp for chronological order
            let mut sorted: Vec<&GroupMsg> = msgs.iter().collect();
            sorted.sort_by_key(|m| m.msg_id);

            let fpath = groups_dir.join(format!("群_{}.txt", gid));
            let mut f = fs::File::create(&fpath).map_err(|e| format!("创建文件失败: {}", e))?;

            // Header
            let first_ts = sorted.first().map(|m| ts_to_str(m.msg_id)).unwrap_or_default();
            let last_ts = sorted.last().map(|m| ts_to_str(m.msg_id)).unwrap_or_default();
            writeln!(f, "{}", "=".repeat(50)).ok();
            if group_name.is_empty() {
                writeln!(f, "群聊：{}", gid).ok();
            } else {
                writeln!(f, "群聊：{} ({})", group_name, gid).ok();
            }
            writeln!(f, "消息数：{} 条", sorted.len()).ok();
            if !first_ts.is_empty() {
                writeln!(f, "时间范围：{} ~ {}", first_ts, last_ts).ok();
            }
            writeln!(f, "{}", "=".repeat(50)).ok();
            writeln!(f).ok();

            // Messages grouped by date
            let mut last_date = String::new();
            for (mi, m) in sorted.iter().enumerate() {
                let ts = m.msg_id;
                let date = format_date(ts);
                let time = format_time(ts);
                if date != last_date {
                    last_date = date.clone();
                    writeln!(f, "———— {} ————", date).ok();
                    writeln!(f).ok();
                }
                let label = if !m.nick.is_empty() { m.nick.clone() }
                    else { store.uid_map.get(&m.uid).cloned().unwrap_or_else(|| m.uid.clone()) };
                let parsed = extract_text(&m.blob);
                let time_str = if time.is_empty() { String::new() } else { format!("[{}] ", time) };
                if let Err(e) = writeln!(f, "{}{}：", time_str, label) {
                    log("error", format!("写入群 {} 消息 {} 失败: {}", gid, mi, e));
                }
                if let Err(e) = writeln!(f, "{}", parsed.content) {
                    log("error", format!("写入群 {} 消息 {} 内容失败: {}", gid, mi, e));
                }
                writeln!(f).ok();
                if (mi + 1) % 1000 == 0 {
                    log("info", format!("  群 {} 进度: {}/{}", gid, mi + 1, sorted.len()));
                }
            }
            total += sorted.len(); group_count += 1;
            files.push(fpath.to_string_lossy().to_string());
        }

        // Filter private chats
        let peer_filter_set: Option<std::collections::HashSet<&str>> = peer_ids_filter
            .map(|f| f.iter().map(|s| s.as_str()).collect());
        // 无筛选条件时不导出（空选 = 不导出）
        let peer_ids: Vec<&String> = match &peer_filter_set {
            None => Vec::new(),
            Some(f) => store.c2c_msgs.keys()
                .filter(|pid| f.contains(pid.as_str()))
                .collect(),
        };

        for peer in &peer_ids {
            if is_cancelled() {
                log("warn", "导出已被取消".to_string());
                return Ok(ExportResult { total, groups: group_count, private: private_count, files });
            }
            let msgs = &store.c2c_msgs[*peer];
            // Build readable label: prefer QQ number, then nickname from messages, fallback to UID
            let qq = store.uid_map.get(*peer);
            let nick = msgs.iter().find(|m| !m.nick.is_empty()).map(|m| m.nick.as_str());
            let label = match (qq, nick) {
                (Some(q), Some(n)) => format!("{}_{}", q, sanitize_filename(n)),
                (Some(q), None) => q.clone(),
                (None, Some(n)) => sanitize_filename(n),
                (None, None) => sanitize_filename(peer),
            };
            log("info", format!("正在导出私聊 {} - {} 条消息...", label, msgs.len()));

            let mut sorted: Vec<&C2cMsg> = msgs.iter().collect();
            sorted.sort_by_key(|m| m.msg_id);

            let fpath = private_dir.join(format!("{}.txt", label));
            let mut f = fs::File::create(&fpath).map_err(|e| format!("创建文件失败: {}", e))?;

            let first_ts = sorted.first().map(|m| ts_to_str(m.msg_id)).unwrap_or_default();
            let last_ts = sorted.last().map(|m| ts_to_str(m.msg_id)).unwrap_or_default();
            writeln!(f, "{}", "=".repeat(50)).ok();
            writeln!(f, "私聊：{}", label).ok();
            writeln!(f, "消息数：{} 条", sorted.len()).ok();
            if !first_ts.is_empty() {
                writeln!(f, "时间范围：{} ~ {}", first_ts, last_ts).ok();
            }
            writeln!(f, "{}", "=".repeat(50)).ok();
            writeln!(f).ok();

            let mut last_date = String::new();
            for (mi, m) in sorted.iter().enumerate() {
                let ts = m.msg_id;
                let date = format_date(ts);
                let time = format_time(ts);
                if date != last_date {
                    last_date = date.clone();
                    writeln!(f, "———— {} ————", date).ok();
                    writeln!(f).ok();
                }
                let who = if !m.nick.is_empty() { m.nick.clone() }
                    else { store.uid_map.get(&m.peer).cloned().unwrap_or_else(|| m.peer.clone()) };
                let parsed = extract_text(&m.blob);
                let time_str = if time.is_empty() { String::new() } else { format!("[{}] ", time) };
                if let Err(e) = writeln!(f, "{}{}：", time_str, who) {
                    log("error", format!("写入私聊 {} 消息 {} 失败: {}", label, mi, e));
                }
                if let Err(e) = writeln!(f, "{}", parsed.content) {
                    log("error", format!("写入私聊 {} 消息 {} 内容失败: {}", label, mi, e));
                }
                writeln!(f).ok();
                if (mi + 1) % 1000 == 0 {
                    log("info", format!("  私聊 {} 进度: {}/{}", label, mi + 1, sorted.len()));
                }
            }
            total += sorted.len(); private_count += 1;
            files.push(fpath.to_string_lossy().to_string());
        }

        return Ok(ExportResult { total, groups: group_count, private: private_count, files });
    }

    // ── Slow path: SQL fallback ──
    let conn = open_db_for_analysis(db_path, key)?;
    let uid_map = load_uid_map(&conn);

    let groups_dir = Path::new(output_dir).join("群聊");
    let private_dir = Path::new(output_dir).join("私聊");
    fs::create_dir_all(&groups_dir).ok();
    fs::create_dir_all(&private_dir).ok();

    let mut total = 0;
    let mut group_count = 0;
    let mut private_count = 0;
    let mut files = Vec::new();

    // Export group chats
    let group_ids: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT DISTINCT \"40021\" FROM group_msg_table")
            .unwrap_or_else(|_| conn.prepare("SELECT 1 WHERE 0").unwrap());
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap();
        rows.flatten().collect()
    };

    let mut sorted_groups = group_ids;
    sorted_groups.sort();

    // 无筛选条件时不导出（空选 = 不导出）
    match group_ids_filter {
        Some(filter) if !filter.is_empty() => {
            let filter_set: std::collections::HashSet<&str> = filter.iter().map(|s| s.as_str()).collect();
            sorted_groups.retain(|g| filter_set.contains(g.as_str()));
        }
        _ => sorted_groups.clear(),
    }

    let total_groups = sorted_groups.len();

    for (idx, gid) in sorted_groups.iter().enumerate() {
        log("info", format!("正在导出群聊 ({}/{})...", idx + 1, total_groups));

        let mut stmt = conn
            .prepare(
                "SELECT \"40001\", \"40020\", \"40093\", \"40800\"
                 FROM group_msg_table WHERE \"40021\" = ?1 ",
            )
            .map_err(|e| format!("查询群聊失败: {}", e))?;

        let fpath = groups_dir.join(format!("群_{}.txt", gid));
        let mut f =
            fs::File::create(&fpath).map_err(|e| format!("创建文件失败: {}", e))?;

        writeln!(f, "群 {}", gid).ok();
        writeln!(f, "{}", "=".repeat(60)).ok();
        writeln!(f).ok();

        let mut group_total = 0usize;
        let rows = stmt
            .query_map(rusqlite::params![gid], |row| {
                Ok((
                    row.get::<_, i64>(0).unwrap_or(0),
                    row.get::<_, String>(1).unwrap_or_default(),
                    row.get::<_, String>(2).unwrap_or_default(),
                    row.get::<_, Vec<u8>>(3).unwrap_or_default(),
                ))
            })
            .map_err(|e| format!("读取群聊消息失败: {}", e))?;

        for row in rows.flatten() {
            let (_msg_id, uid, nick, blob) = row;
            let label = if !nick.is_empty() {
                nick
            } else {
                uid_map.get(uid.as_str()).cloned().unwrap_or_else(|| uid.to_string())
            };
            let parsed = extract_text(&blob);
            writeln!(f, "[{}] {}", label, parsed.content).ok();
            group_total += 1;
            if group_total % 1000 == 0 {
                log("info", format!("  群 {} 已导出 {} 条...", gid, group_total));
            }
        }

        if group_total == 0 {
            continue;
        }

        writeln!(f, "\n--- 共 {} 条消息 ---", group_total).ok();

        total += group_total;
        group_count += 1;
        files.push(fpath.to_string_lossy().to_string());
    }

    // Export private chats
    let peer_uids: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT DISTINCT \"40020\" FROM c2c_msg_table")
            .unwrap_or_else(|_| conn.prepare("SELECT 1 WHERE 0").unwrap());
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap();
        rows.flatten().collect()
    };

    let mut sorted_peers = peer_uids;
    sorted_peers.sort();

    // 无筛选条件时不导出（空选 = 不导出）
    match peer_ids_filter {
        Some(filter) if !filter.is_empty() => {
            let fs: std::collections::HashSet<&str> = filter.iter().map(|s| s.as_str()).collect();
            sorted_peers.retain(|p| fs.contains(p.as_str()));
        }
        _ => sorted_peers.clear(),
    }

    let total_peers = sorted_peers.len();

    for (idx, uid) in sorted_peers.iter().enumerate() {
        log("info", format!("正在导出私聊 ({}/{})...", idx + 1, total_peers));
        let mut stmt = conn
            .prepare(
                "SELECT \"40001\", \"40020\", \"40093\", \"40800\"
                 FROM c2c_msg_table WHERE \"40020\" = ?1 ",
            )
            .map_err(|e| format!("查询私聊失败: {}", e))?;

        let qq = uid_map.get(uid.as_str());
        let label = match qq {
            Some(q) => q.clone(),
            None => sanitize_filename(uid),
        };
        let fpath = private_dir.join(format!("{}.txt", label));
        let mut f =
            fs::File::create(&fpath).map_err(|e| format!("创建文件失败: {}", e))?;

        writeln!(f, "与 {} 的私聊记录", label).ok();
        writeln!(f, "{}", "=".repeat(60)).ok();
        writeln!(f).ok();

        let mut peer_total = 0usize;
        let rows = stmt
            .query_map(rusqlite::params![uid], |row| {
                Ok((
                    row.get::<_, i64>(0).unwrap_or(0),
                    row.get::<_, String>(1).unwrap_or_default(),
                    row.get::<_, String>(2).unwrap_or_default(),
                    row.get::<_, Vec<u8>>(3).unwrap_or_default(),
                ))
            })
            .map_err(|e| format!("读取私聊消息失败: {}", e))?;

        for row in rows.flatten() {
            let (_msg_id, peer, nick, blob) = row;
            let who = if !nick.is_empty() {
                nick
            } else {
                uid_map.get(peer.as_str()).cloned().unwrap_or_else(|| peer.to_string())
            };
            let parsed = extract_text(&blob);
            writeln!(f, "[{}] {}", who, parsed.content).ok();
            peer_total += 1;
        }

        if peer_total == 0 {
            continue;
        }

        writeln!(f, "\n--- 共 {} 条消息 ---", peer_total).ok();

        total += peer_total;
        private_count += 1;
        files.push(fpath.to_string_lossy().to_string());
    }

    Ok(ExportResult {
        total,
        groups: group_count,
        private: private_count,
        files,
    })
}
