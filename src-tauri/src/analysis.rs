/// Chat analysis module.
/// Provides group and private chat statistics, CSV export, and analysis data.
/// Supports both in-memory MessageStore (fast) and direct SQL queries (fallback).

use crate::message_parser::extract_text;
use chrono::{Datelike, NaiveDateTime, Timelike, Weekday};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupInfo {
    pub id: String,
    pub name: String,
    pub message_count: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberStat {
    pub name: String,
    pub message_count: i64,
    pub first_message: String,
    pub last_message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhraseStat {
    pub phrase: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeDistItem {
    pub name: String,
    pub value: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupAnalysisData {
    pub group_id: String,
    pub total_messages: usize,
    pub member_count: usize,
    pub member_ranking: Vec<MemberStat>,
    pub hourly_distribution: HashMap<String, i64>,
    pub weekday_distribution: HashMap<String, i64>,
    pub monthly_distribution: HashMap<String, i64>,
    pub type_distribution: Vec<TypeDistItem>,
    pub top_phrases: Vec<PhraseStat>,
    pub first_message: String,
    pub last_message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateAnalysisData {
    pub total_messages: usize,
    pub active_days: usize,
    pub contact_count: usize,
    pub contact_ranking: Vec<MemberStat>,
    pub hourly_distribution: HashMap<String, i64>,
    pub weekday_distribution: HashMap<String, i64>,
    pub monthly_distribution: HashMap<String, i64>,
    pub type_distribution: Vec<TypeDistItem>,
    pub top_phrases: Vec<PhraseStat>,
    pub first_message: String,
    pub last_message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupListData {
    pub groups: Vec<GroupInfo>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvExportData {
    pub groups: usize,
    pub private: usize,
    pub total: usize,
    pub files: Vec<String>,
}

// ── helpers ──

pub fn normalize_ts(ts: i64) -> i64 {
    if ts == 0 { return 0 }
    if ts > 1_000_000_000_000_000_000 { ts / 1_000_000_000 }
    else if ts > 1_000_000_000_000 { ts / 1_000 }
    else { ts }
}

pub fn ts_to_str(ts: i64) -> String {
    let secs = normalize_ts(ts);
    if secs == 0 { return String::new() }
    NaiveDateTime::from_timestamp_opt(secs, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default()
}

fn ts_to_hour(ts: i64) -> i32 {
    let secs = normalize_ts(ts);
    if secs == 0 { return -1 }
    NaiveDateTime::from_timestamp_opt(secs, 0)
        .map(|dt| dt.time().hour() as i32)
        .unwrap_or(-1)
}

fn ts_to_weekday(ts: i64) -> i32 {
    let secs = normalize_ts(ts);
    if secs == 0 { return -1 }
    NaiveDateTime::from_timestamp_opt(secs, 0)
        .map(|dt| match dt.weekday() {
            Weekday::Mon => 0, Weekday::Tue => 1, Weekday::Wed => 2,
            Weekday::Thu => 3, Weekday::Fri => 4, Weekday::Sat => 5, Weekday::Sun => 6,
        })
        .unwrap_or(-1)
}

fn ts_to_month(ts: i64) -> String {
    let secs = normalize_ts(ts);
    if secs == 0 { return String::new() }
    NaiveDateTime::from_timestamp_opt(secs, 0)
        .map(|dt| dt.format("%Y-%m").to_string())
        .unwrap_or_default()
}

fn ts_to_date(ts: i64) -> String {
    let secs = normalize_ts(ts);
    if secs == 0 { return String::new() }
    NaiveDateTime::from_timestamp_opt(secs, 0)
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

fn weekday_name(i: i32) -> String {
    match i {
        0 => "周一", 1 => "周二", 2 => "周三", 3 => "周四",
        4 => "周五", 5 => "周六", 6 => "周日", _ => "?",
    }.to_string()
}

fn build_type_distribution(counts: &HashMap<String, i64>) -> Vec<TypeDistItem> {
    let types = ["text", "image", "voice", "video", "system", "miniapp", "other"];
    let names = ["文本", "图片", "语音", "视频", "系统", "小程序", "其他"];
    types.iter().zip(names.iter())
        .map(|(k, n)| TypeDistItem { name: n.to_string(), value: *counts.get(*k).unwrap_or(&0) })
        .collect()
}

fn classify_phrases(content: &str, msg_type: &str, phrases: &mut HashMap<String, i64>) {
    if msg_type != "text" { return; }
    let len = content.chars().count();
    if !(2..=20).contains(&len) { return; }
    if content.contains("http") || content.contains("www.")
        || content.contains('<') || content.contains('>')
        || content.contains('{') || content.contains('}')
    { return; }
    *phrases.entry(content.to_string()).or_insert(0) += 1;
}

// ── public API ──

/// List all groups with message counts. Uses MessageStore if available.
pub fn list_groups(
    db_path: &str, key: &str,
    store: Option<&super::export_chat::MessageStore>,
) -> Result<GroupListData, String> {
    // Fast path: use in-memory store
    if let Some(store) = store {
        let groups: Vec<GroupInfo> = store.group_msgs.iter().map(|(id, msgs)| {
            GroupInfo {
                id: id.clone(),
                name: store.group_names.get(id).cloned().unwrap_or_default(),
                message_count: msgs.len() as i64,
            }
        }).collect();
        return Ok(GroupListData { groups });
    }

    // Slow path: SQL query
    let conn = super::export_chat::open_db_for_analysis(db_path, key)?;
    let group_names = super::export_chat::load_group_names(&conn);

    let mut stmt = conn
        .prepare("SELECT \"40021\", count(*) FROM group_msg_table GROUP BY \"40021\"")
        .map_err(|e| e.to_string())?;

    let groups: Vec<GroupInfo> = stmt
        .query_map([], |row| Ok(GroupInfo {
            id: row.get(0)?, message_count: row.get(1)?, name: String::new(),
        }))
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|mut g| { g.name = group_names.get(&g.id).cloned().unwrap_or_default(); g })
        .collect();

    Ok(GroupListData { groups })
}

/// Analyze a specific group chat. Uses MessageStore if provided (O(1) HashMap lookup),
/// falls back to direct SQL otherwise.
pub fn analyze_group_detail(
    db_path: &str,
    key: &str,
    group_id: &str,
    store: Option<&super::export_chat::MessageStore>,
    progress_log: Option<&std::sync::Mutex<Vec<crate::commands::LogMessage>>>,
) -> Result<GroupAnalysisData, String> {
    let log = |_text: String| {
        if let Some(log) = progress_log {
            if let Ok(mut l) = log.lock() {
                l.push(crate::commands::LogMessage { level: "info".to_string(), text: _text });
            }
        }
    };

    // ── Fast path: in-memory store ──
    if let Some(store) = store {
        let msgs = store.group_msgs.get(group_id).ok_or("未找到群聊数据".to_string())?;
        let uid_map = &store.uid_map;

        let mut member_counts: HashMap<String, i64> = HashMap::new();
        let mut member_first: HashMap<String, String> = HashMap::new();
        let mut member_last: HashMap<String, String> = HashMap::new();
        let mut hourly = HashMap::new();
        let mut weekday_counts = HashMap::new();
        let mut monthly = HashMap::new();
        let mut type_counts = HashMap::new();
        let mut phrases = HashMap::new();
        let total = msgs.len();
        let mut first_id = i64::MAX;
        let mut last_id = i64::MIN;

        for m in msgs {
            if m.msg_id < first_id { first_id = m.msg_id; }
            if m.msg_id > last_id { last_id = m.msg_id; }
            let sender = if !m.nick.is_empty() { m.nick.clone() }
                else { uid_map.get(&m.uid).cloned().unwrap_or_else(|| m.uid.clone()) };
            let parsed = extract_text(&m.blob);
            *member_counts.entry(sender.clone()).or_insert(0) += 1;
            let h = ts_to_hour(m.msg_id); if h >= 0 { *hourly.entry(h).or_insert(0) += 1; }
            let w = ts_to_weekday(m.msg_id); if w >= 0 { *weekday_counts.entry(w).or_insert(0) += 1; }
            let mo = ts_to_month(m.msg_id); if !mo.is_empty() { *monthly.entry(mo).or_insert(0) += 1; }
            *type_counts.entry(parsed.msg_type.clone()).or_insert(0) += 1;
            let ts = ts_to_str(m.msg_id);
            if !ts.is_empty() {
                member_first.entry(sender.clone()).or_insert_with(|| ts.clone());
                member_last.insert(sender, ts);
            }
            classify_phrases(&parsed.content, &parsed.msg_type, &mut phrases);
        }

        if total == 0 { return Err("未找到群聊数据".to_string()); }
        return build_group_result(group_id, total, &member_counts, &member_first, &member_last,
            &hourly, &weekday_counts, &monthly, &type_counts, &phrases, first_id, last_id);
    }

    // ── Slow path: SQL fallback ──
    log("(SQL fallback) 打开数据库...".to_string());
    let conn = super::export_chat::open_db_for_analysis(db_path, key)?;
    let uid_map = super::export_chat::load_uid_map(&conn);

    let mut stmt = conn.prepare(
        "SELECT \"40001\", \"40020\", \"40093\", \"40800\" FROM group_msg_table WHERE \"40021\" = ?1"
    ).map_err(|e| format!("SQL错误: {}", e))?;

    let mut member_counts = HashMap::new();
    let mut member_first = HashMap::new();
    let mut member_last = HashMap::new();
    let mut hourly = HashMap::new();
    let mut weekday_counts = HashMap::new();
    let mut monthly = HashMap::new();
    let mut type_counts = HashMap::new();
    let mut phrases = HashMap::new();
    let mut total = 0usize;
    let mut first_id = i64::MAX;
    let mut last_id = i64::MIN;

    let rows = stmt.query_map(rusqlite::params![group_id], |row| Ok((
        row.get::<_, i64>(0).unwrap_or(0),
        row.get::<_, String>(1).unwrap_or_default(),
        row.get::<_, String>(2).unwrap_or_default(),
        row.get::<_, Vec<u8>>(3).unwrap_or_default(),
    ))).map_err(|e| format!("查询失败: {}", e))?;

    for row in rows {
        let (msg_id, uid, nick, blob) = row.map_err(|e| e.to_string())?;
        if msg_id < first_id { first_id = msg_id; }
        if msg_id > last_id { last_id = msg_id; }
        total += 1;
        let sender = if !nick.is_empty() { nick } else { uid_map.get(uid.as_str()).cloned().unwrap_or(uid) };
        let parsed = extract_text(&blob);
        *member_counts.entry(sender.clone()).or_insert(0) += 1;
        let h = ts_to_hour(msg_id); if h >= 0 { *hourly.entry(h).or_insert(0) += 1; }
        let w = ts_to_weekday(msg_id); if w >= 0 { *weekday_counts.entry(w).or_insert(0) += 1; }
        let mo = ts_to_month(msg_id); if !mo.is_empty() { *monthly.entry(mo).or_insert(0) += 1; }
        *type_counts.entry(parsed.msg_type.clone()).or_insert(0) += 1;
        let ts = ts_to_str(msg_id);
        if !ts.is_empty() { member_first.entry(sender.clone()).or_insert_with(|| ts.clone()); member_last.insert(sender, ts); }
        classify_phrases(&parsed.content, &parsed.msg_type, &mut phrases);
    }
    if total == 0 { return Err("未找到群聊数据".to_string()); }
    build_group_result(group_id, total, &member_counts, &member_first, &member_last,
        &hourly, &weekday_counts, &monthly, &type_counts, &phrases, first_id, last_id)
}

/// Shared result builder for group analysis (used by both fast and slow paths).
fn build_group_result(
    group_id: &str, total: usize,
    member_counts: &HashMap<String, i64>,
    member_first: &HashMap<String, String>, member_last: &HashMap<String, String>,
    hourly: &HashMap<i32, i64>, weekday_counts: &HashMap<i32, i64>,
    monthly: &HashMap<String, i64>, type_counts: &HashMap<String, i64>,
    phrases: &HashMap<String, i64>, first_id: i64, last_id: i64,
) -> Result<GroupAnalysisData, String> {
    let mut ranking: Vec<(String, i64)> = member_counts.iter().map(|(k, v)| (k.clone(), *v)).collect();
    ranking.sort_by(|a, b| b.1.cmp(&a.1));
    let member_ranking: Vec<MemberStat> = ranking.iter().take(50).map(|(name, count)| MemberStat {
        name: name.clone(), message_count: *count,
        first_message: member_first.get(name).cloned().unwrap_or_default(),
        last_message: member_last.get(name).cloned().unwrap_or_default(),
    }).collect();
    let hourly_dist: HashMap<String, i64> = (0..24).map(|h| (h.to_string(), *hourly.get(&h).unwrap_or(&0))).collect();
    let weekday_dist: HashMap<String, i64> = (0..7).map(|w| (weekday_name(w), *weekday_counts.get(&w).unwrap_or(&0))).collect();
    let type_dist = build_type_distribution(type_counts);
    let mut pv: Vec<(String, i64)> = phrases.iter().map(|(k, v)| (k.clone(), *v)).collect();
    pv.sort_by(|a, b| b.1.cmp(&a.1));
    let top_phrases: Vec<PhraseStat> = pv.into_iter().filter(|(_, c)| *c >= 2).take(20)
        .map(|(phrase, count)| PhraseStat { phrase, count }).collect();
    Ok(GroupAnalysisData {
        group_id: group_id.to_string(), total_messages: total,
        member_count: member_ranking.len(), member_ranking,
        hourly_distribution: hourly_dist, weekday_distribution: weekday_dist,
        monthly_distribution: monthly.iter().map(|(k, v)| (k.clone(), *v)).collect(),
        type_distribution: type_dist, top_phrases,
        first_message: ts_to_str(first_id), last_message: ts_to_str(last_id),
    })
}

/// Analyze a single contact's chat. Uses MessageStore if provided.
pub fn analyze_private_detail(
    db_path: &str, key: &str,
    peer_uid: &str,
    store: Option<&super::export_chat::MessageStore>,
    progress_log: Option<&std::sync::Mutex<Vec<crate::commands::LogMessage>>>,
) -> Result<PrivateAnalysisData, String> {
    let log = |text: String| {
        if let Some(log) = progress_log {
            if let Ok(mut l) = log.lock() {
                l.push(crate::commands::LogMessage { level: "info".to_string(), text });
            }
        }
    };

    if let Some(store) = store {
        let msgs = store.c2c_msgs.get(peer_uid).ok_or("未找到该联系人的聊天数据".to_string())?;
        let uid_map = &store.uid_map;
        log(format!("正在分析私聊 ({} 条消息)...", msgs.len()));

        let mut counts: HashMap<String, i64> = HashMap::new();
        let mut first_ts: HashMap<String, String> = HashMap::new();
        let mut last_ts: HashMap<String, String> = HashMap::new();
        let mut hourly = HashMap::new();
        let mut wday = HashMap::new();
        let mut monthly = HashMap::new();
        let mut types = HashMap::new();
        let mut daily = HashMap::new();
        let mut phrases = HashMap::new();
        let total = msgs.len();
        let mut first_id = i64::MAX;
        let mut last_id = i64::MIN;

        for m in msgs {
            if m.msg_id < first_id { first_id = m.msg_id; }
            if m.msg_id > last_id { last_id = m.msg_id; }
            let sender = if !m.nick.is_empty() { m.nick.clone() }
                else { uid_map.get(&m.peer).cloned().unwrap_or_else(|| m.peer.clone()) };
            let parsed = extract_text(&m.blob);
            *counts.entry(sender.clone()).or_insert(0) += 1;
            let h = ts_to_hour(m.msg_id); if h >= 0 { *hourly.entry(h).or_insert(0) += 1; }
            let w = ts_to_weekday(m.msg_id); if w >= 0 { *wday.entry(w).or_insert(0) += 1; }
            let mo = ts_to_month(m.msg_id); if !mo.is_empty() { *monthly.entry(mo).or_insert(0) += 1; }
            let d = ts_to_date(m.msg_id); if !d.is_empty() { *daily.entry(d).or_insert(0) += 1; }
            *types.entry(parsed.msg_type.clone()).or_insert(0) += 1;
            let ts = ts_to_str(m.msg_id);
            if !ts.is_empty() { first_ts.entry(sender.clone()).or_insert_with(|| ts.clone()); last_ts.insert(sender, ts); }
            classify_phrases(&parsed.content, &parsed.msg_type, &mut phrases);
        }
        if total == 0 { return Err("未找到聊天数据".to_string()); }
        return build_private_result(total, daily.len(), &counts, &first_ts, &last_ts,
            &hourly, &wday, &monthly, &types, &phrases, first_id, last_id);
    }

    // SQL fallback
    let conn = super::export_chat::open_db_for_analysis(db_path, key)?;
    let uid_map = super::export_chat::load_uid_map(&conn);
    let mut stmt = conn.prepare(
        "SELECT \"40001\", \"40020\", \"40093\", \"40800\" FROM c2c_msg_table WHERE \"40020\" = ?1"
    ).map_err(|e| e.to_string())?;

    let mut counts = HashMap::new(); let mut first_ts = HashMap::new(); let mut last_ts = HashMap::new();
    let mut hourly = HashMap::new(); let mut wday = HashMap::new(); let mut monthly = HashMap::new();
    let mut types = HashMap::new(); let mut daily = HashMap::new(); let mut phrases = HashMap::new();
    let mut total = 0usize; let mut first_id = i64::MAX; let mut last_id = i64::MIN;

    let rows = stmt.query_map(rusqlite::params![peer_uid], |row| Ok((
        row.get::<_, i64>(0).unwrap_or(0), row.get::<_, String>(1).unwrap_or_default(),
        row.get::<_, String>(2).unwrap_or_default(), row.get::<_, Vec<u8>>(3).unwrap_or_default(),
    ))).map_err(|e| e.to_string())?;

    for row in rows {
        let (msg_id, peer, nick, blob) = match row { Ok(r) => r, Err(_) => continue };
        if msg_id < first_id { first_id = msg_id; } if msg_id > last_id { last_id = msg_id; }
        total += 1;
        let sender = if !nick.is_empty() { nick } else { uid_map.get(peer.as_str()).cloned().unwrap_or(peer) };
        let parsed = extract_text(&blob);
        *counts.entry(sender.clone()).or_insert(0) += 1;
        let h = ts_to_hour(msg_id); if h >= 0 { *hourly.entry(h).or_insert(0) += 1; }
        let w = ts_to_weekday(msg_id); if w >= 0 { *wday.entry(w).or_insert(0) += 1; }
        let mo = ts_to_month(msg_id); if !mo.is_empty() { *monthly.entry(mo).or_insert(0) += 1; }
        let d = ts_to_date(msg_id); if !d.is_empty() { *daily.entry(d).or_insert(0) += 1; }
        *types.entry(parsed.msg_type.clone()).or_insert(0) += 1;
        let ts = ts_to_str(msg_id);
        if !ts.is_empty() { first_ts.entry(sender.clone()).or_insert_with(|| ts.clone()); last_ts.insert(sender, ts); }
        classify_phrases(&parsed.content, &parsed.msg_type, &mut phrases);
    }
    if total == 0 { return Err("未找到聊天数据".to_string()); }
    build_private_result(total, daily.len(), &counts, &first_ts, &last_ts,
        &hourly, &wday, &monthly, &types, &phrases, first_id, last_id)
}

/// List all contacts with message counts. Uses MessageStore if provided.
pub fn list_contacts(
    db_path: &str, key: &str,
    store: Option<&super::export_chat::MessageStore>,
) -> Result<GroupListData, String> {
    if let Some(store) = store {
        let groups: Vec<GroupInfo> = store.c2c_msgs.iter().map(|(id, msgs)| {
            let name = store.uid_map.get(id).cloned().unwrap_or_else(|| id.clone());
            GroupInfo { id: id.clone(), name, message_count: msgs.len() as i64 }
        }).collect();
        return Ok(GroupListData { groups });
    }
    // SQL fallback
    let conn = super::export_chat::open_db_for_analysis(db_path, key)?;
    let uid_map = super::export_chat::load_uid_map(&conn);
    let mut stmt = conn.prepare(
        "SELECT \"40020\", count(*) FROM c2c_msg_table GROUP BY \"40020\""
    ).map_err(|e| e.to_string())?;
    let groups: Vec<GroupInfo> = stmt.query_map([], |row| Ok(GroupInfo {
        id: row.get(0)?, name: String::new(), message_count: row.get(1)?,
    })).map_err(|e| e.to_string())?.flatten()
        .map(|mut g| { g.name = uid_map.get(&g.id).cloned().unwrap_or_default(); g }).collect();
    Ok(GroupListData { groups })
}

fn build_private_result(
    total: usize, active_days: usize,
    contact_counts: &HashMap<String, i64>,
    contact_first: &HashMap<String, String>, contact_last: &HashMap<String, String>,
    hourly: &HashMap<i32, i64>, weekday_counts: &HashMap<i32, i64>,
    monthly: &HashMap<String, i64>, type_counts: &HashMap<String, i64>,
    phrases: &HashMap<String, i64>, first_id: i64, last_id: i64,
) -> Result<PrivateAnalysisData, String> {
    let mut ranking: Vec<(String, i64)> = contact_counts.iter().map(|(k, v)| (k.clone(), *v)).collect();
    ranking.sort_by(|a, b| b.1.cmp(&a.1));
    let contact_ranking: Vec<MemberStat> = ranking.iter().take(50).map(|(name, count)| MemberStat {
        name: name.clone(), message_count: *count,
        first_message: contact_first.get(name).cloned().unwrap_or_default(),
        last_message: contact_last.get(name).cloned().unwrap_or_default(),
    }).collect();
    let hourly_dist: HashMap<String, i64> = (0..24).map(|h| (h.to_string(), *hourly.get(&h).unwrap_or(&0))).collect();
    let weekday_dist: HashMap<String, i64> = (0..7).map(|w| (weekday_name(w), *weekday_counts.get(&w).unwrap_or(&0))).collect();
    let type_dist = build_type_distribution(type_counts);
    let mut pv: Vec<(String, i64)> = phrases.iter().map(|(k, v)| (k.clone(), *v)).collect();
    pv.sort_by(|a, b| b.1.cmp(&a.1));
    let top_phrases: Vec<PhraseStat> = pv.into_iter().filter(|(_, c)| *c >= 2).take(20)
        .map(|(phrase, count)| PhraseStat { phrase, count }).collect();
    Ok(PrivateAnalysisData {
        total_messages: total, active_days,
        contact_count: contact_ranking.len(), contact_ranking,
        hourly_distribution: hourly_dist, weekday_distribution: weekday_dist,
        monthly_distribution: monthly.iter().map(|(k, v)| (k.clone(), *v)).collect(),
        type_distribution: type_dist, top_phrases,
        first_message: ts_to_str(first_id), last_message: ts_to_str(last_id),
    })
}

/// Export CSV files. Uses MessageStore if provided.
pub fn export_csv(
    db_path: &str, key: &str, output_dir: &str,
    group_ids_filter: Option<&[String]>,
    peer_ids_filter: Option<&[String]>,
    store: Option<&super::export_chat::MessageStore>,
    progress_log: Option<&std::sync::Mutex<Vec<crate::commands::LogMessage>>>,
    cancel_flag: Option<&std::sync::atomic::AtomicBool>,
) -> Result<CsvExportData, String> {
    use std::fs;
    use std::sync::atomic::Ordering;

    let is_cancelled = || -> bool {
        cancel_flag.map_or(false, |f| f.load(Ordering::SeqCst))
    };

    let log = |text: String| {
        if let Some(log) = progress_log {
            if let Ok(mut l) = log.lock() {
                l.push(crate::commands::LogMessage { level: "info".to_string(), text });
            }
        }
    };

    fs::create_dir_all(output_dir).map_err(|e| e.to_string())?;
    log("正在导出 CSV...".to_string());

    // ── Fast path: in-memory store ──
    if let Some(store) = store {
        let mut total = 0usize;
        let mut group_count = 0usize;
        let mut private_count = 0usize;
        let mut files = Vec::new();
        let uid_map = &store.uid_map;

        let groups_dir = std::path::Path::new(output_dir).join("群聊表格");
        fs::create_dir_all(&groups_dir).ok();
        let filter_set: Option<std::collections::HashSet<&str>> = group_ids_filter
            .map(|f| f.iter().map(|s| s.as_str()).collect());
        // 无筛选条件时不导出（空选 = 不导出）
        let group_ids: Vec<&String> = match &filter_set {
            None => Vec::new(),
            Some(f) => store.group_msgs.keys()
                .filter(|gid| f.contains(gid.as_str()))
                .collect(),
        };

        for gid in &group_ids {
            if is_cancelled() { return Ok(CsvExportData { groups: group_count, private: private_count, total, files }); }
            let msgs = &store.group_msgs[*gid];
            let csv_path = groups_dir.join(format!("群_{}.csv", gid));
            let mut file = std::fs::File::create(&csv_path).map_err(|e| e.to_string())?;
            use std::io::Write;
            file.write_all(&[0xEF, 0xBB, 0xBF]).map_err(|e| e.to_string())?;
            let mut wtr = csv::Writer::from_writer(file);
            wtr.write_record(["时间", "发送者", "类型", "内容"]).map_err(|e| e.to_string())?;
            for m in msgs {
                let sender = if !m.nick.is_empty() { &m.nick } else { uid_map.get(&m.uid).map(|s| s.as_str()).unwrap_or(&m.uid) };
                let parsed = extract_text(&m.blob);
                wtr.write_record([&ts_to_str(m.msg_id), sender, &parsed.msg_type, &parsed.content])
                    .map_err(|e| e.to_string())?;
            }
            wtr.flush().map_err(|e| format!("CSV写入失败: {}", e))?;
            if !msgs.is_empty() { total += msgs.len(); group_count += 1; files.push(csv_path.to_string_lossy().to_string()); }
        }

        let private_dir = std::path::Path::new(output_dir).join("私聊表格");
        fs::create_dir_all(&private_dir).ok();
        let peer_filter: Option<std::collections::HashSet<&str>> = peer_ids_filter
            .map(|f| f.iter().map(|s| s.as_str()).collect());
        // 无筛选条件时不导出（空选 = 不导出）
        for (peer, msgs) in &store.c2c_msgs {
            match &peer_filter {
                None => continue,
                Some(f) if !f.contains(peer.as_str()) => continue,
                _ => {}
            }
            let qq = uid_map.get(peer);
            let nick = msgs.iter().find(|m| !m.nick.is_empty()).map(|m| m.nick.as_str());
            let label = match (qq, nick) {
                (Some(q), Some(n)) => format!("{}_{}", q, n),
                (Some(q), None) => q.clone(),
                (None, Some(n)) => n.to_string(),
                (None, None) => peer.clone(),
            };
            let csv_path = private_dir.join(format!("{}.csv", crate::export_chat::sanitize_filename(&label)));
            let mut file = std::fs::File::create(&csv_path).map_err(|e| e.to_string())?;
            use std::io::Write;
            file.write_all(&[0xEF, 0xBB, 0xBF]).map_err(|e| e.to_string())?;
            let mut wtr = csv::Writer::from_writer(file);
            wtr.write_record(["时间", "发送者", "类型", "内容"]).map_err(|e| e.to_string())?;
            for m in msgs {
                let sender = if !m.nick.is_empty() { &m.nick } else { uid_map.get(&m.peer).map(|s| s.as_str()).unwrap_or(&m.peer) };
                let parsed = extract_text(&m.blob);
                wtr.write_record([&ts_to_str(m.msg_id), sender, &parsed.msg_type, &parsed.content])
                    .map_err(|e| e.to_string())?;
            }
            wtr.flush().map_err(|e| format!("CSV写入失败: {}", e))?;
            if !msgs.is_empty() { total += msgs.len(); private_count += 1; files.push(csv_path.to_string_lossy().to_string()); }
        }

        return Ok(CsvExportData { groups: group_count, private: private_count, total, files });
    }

    // ── Slow path: SQL fallback ──
    let conn = super::export_chat::open_db_for_analysis(db_path, key)?;
    let uid_map = super::export_chat::load_uid_map(&conn);
    let mut total = 0usize;
    let mut group_count = 0usize;
    let mut private_count = 0usize;
    let mut files = Vec::new();

    let groups_dir = std::path::Path::new(output_dir).join("群聊表格");
    fs::create_dir_all(&groups_dir).ok();
    let mut group_ids: Vec<String> = {
        let mut stmt = conn.prepare("SELECT DISTINCT \"40021\" FROM group_msg_table")
            .unwrap_or_else(|_| conn.prepare("SELECT 1 WHERE 0").unwrap());
        stmt.query_map([], |row| row.get(0)).unwrap().flatten().collect()
    };
    // 无筛选条件时不导出（空选 = 不导出）
    match group_ids_filter {
        Some(filter) if !filter.is_empty() => {
            let fs: std::collections::HashSet<&str> = filter.iter().map(|s| s.as_str()).collect();
            group_ids.retain(|g| fs.contains(g.as_str()));
        }
        _ => group_ids.clear(),
    }

    for gid in &group_ids {
        let mut stmt = conn.prepare(
            "SELECT \"40001\", \"40020\", \"40093\", \"40800\" FROM group_msg_table WHERE \"40021\" = ?1"
        ).map_err(|e| e.to_string())?;
        let csv_path = groups_dir.join(format!("群_{}.csv", gid));
        let mut file = std::fs::File::create(&csv_path).map_err(|e| e.to_string())?;
        use std::io::Write;
        file.write_all(&[0xEF, 0xBB, 0xBF]).map_err(|e| e.to_string())?;
        let mut wtr = csv::Writer::from_writer(file);
        wtr.write_record(["时间", "发送者", "类型", "内容"]).map_err(|e| e.to_string())?;
        let mut gt = 0usize;
        let rows = stmt.query_map(rusqlite::params![gid], |row| Ok((
            row.get::<_, i64>(0).unwrap_or(0), row.get::<_, String>(1).unwrap_or_default(),
            row.get::<_, String>(2).unwrap_or_default(), row.get::<_, Vec<u8>>(3).unwrap_or_default(),
        ))).map_err(|e| e.to_string())?;
        for row in rows.flatten() {
            let (msg_id, uid, nick, blob) = row;
            let sender = if !nick.is_empty() { nick } else { uid_map.get(uid.as_str()).cloned().unwrap_or(uid) };
            let parsed = extract_text(&blob);
            wtr.write_record([&ts_to_str(msg_id), &sender, &parsed.msg_type, &parsed.content]).map_err(|e| e.to_string())?;
            gt += 1;
        }
        wtr.flush().map_err(|e| format!("CSV写入失败: {}", e))?;
        if gt > 0 { total += gt; group_count += 1; files.push(csv_path.to_string_lossy().to_string()); }
    }

    let private_dir = std::path::Path::new(output_dir).join("私聊表格");
    fs::create_dir_all(&private_dir).ok();
    let peer_uids: Vec<String> = {
        let mut stmt = conn.prepare("SELECT DISTINCT \"40020\" FROM c2c_msg_table")
            .unwrap_or_else(|_| conn.prepare("SELECT 1 WHERE 0").unwrap());
        stmt.query_map([], |row| row.get(0)).unwrap().flatten().collect()
    };
    // 无筛选条件时不导出（空选 = 不导出）
    let peer_uids: Vec<String> = match peer_ids_filter {
        Some(filter) if !filter.is_empty() => {
            let fs: std::collections::HashSet<&str> = filter.iter().map(|s| s.as_str()).collect();
            peer_uids.into_iter().filter(|u| fs.contains(u.as_str())).collect()
        }
        _ => Vec::new(),
    };
    for uid in &peer_uids {
        let mut stmt = conn.prepare(
            "SELECT \"40001\", \"40020\", \"40093\", \"40800\" FROM c2c_msg_table WHERE \"40020\" = ?1"
        ).map_err(|e| e.to_string())?;
        let label = uid_map.get(uid).cloned().unwrap_or_else(|| uid.clone());
        let csv_path = private_dir.join(format!("{}.csv", label));
        let mut file = std::fs::File::create(&csv_path).map_err(|e| e.to_string())?;
        use std::io::Write;
        file.write_all(&[0xEF, 0xBB, 0xBF]).map_err(|e| e.to_string())?;
        let mut wtr = csv::Writer::from_writer(file);
        wtr.write_record(["时间", "发送者", "类型", "内容"]).map_err(|e| e.to_string())?;
        let mut pt = 0usize;
        let rows = stmt.query_map(rusqlite::params![uid], |row| Ok((
            row.get::<_, i64>(0).unwrap_or(0), row.get::<_, String>(1).unwrap_or_default(),
            row.get::<_, String>(2).unwrap_or_default(), row.get::<_, Vec<u8>>(3).unwrap_or_default(),
        ))).map_err(|e| e.to_string())?;
        for row in rows.flatten() {
            let (msg_id, peer, nick, blob) = row;
            let sender = if !nick.is_empty() { nick } else { uid_map.get(peer.as_str()).cloned().unwrap_or(peer) };
            let parsed = extract_text(&blob);
            wtr.write_record([&ts_to_str(msg_id), &sender, &parsed.msg_type, &parsed.content]).map_err(|e| e.to_string())?;
            pt += 1;
        }
        wtr.flush().map_err(|e| format!("CSV写入失败: {}", e))?;
        if pt > 0 { total += pt; private_count += 1; files.push(csv_path.to_string_lossy().to_string()); }
    }

    Ok(CsvExportData { groups: group_count, private: private_count, total, files })
}
