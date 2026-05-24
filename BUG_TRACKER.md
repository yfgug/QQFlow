# QQFlow-Rust 问题追踪

> 最后更新：2026-05-24 | 当前版本：v1.1.5

---

## 问题总览

> 回归风险定义：**低** = 修复方式单一、不依赖外部状态、未来代码改动几乎不可能重新触发；**中** = 涉及缓存/锁/异步调度，未来改动可能重新引入；**高** = 当前方案有已知缺点（如首次加载耗时长），可能在压力下被替换并引入新问题。

| # | 问题 | 状态 | 修复版本 | 回归风险 |
|---|------|------|----------|----------|
| 1 | 分析命令死锁/永久加载 | ✅ 已解决 | v1.1.2 → v1.1.3 → v1.1.4 | 中（MessageStore 缓存模式多次变更） |
| 2 | 导出缓慢 + 群聊无内容 + UID 显示原始 ID | ✅ 已解决 | v1.1.2 → v1.1.3 → v1.1.5 | 低 |
| 3 | 导出进度不反馈 | ✅ 已解决 | v1.1.2 → v1.1.4 | 低 |
| 4 | BLOB 解析含 Protobuf 垃圾 / JSON 元数据 | ✅ 已解决 | v1.1.1 → v1.1.3 → v1.1.5 | 低 |
| 5 | `extract_text` 文本提取不全 | ✅ 已解决 | v1.1.3 | 低 |
| 6 | UID 映射列检测错误 | ✅ 已解决 | v1.1.3 → v1.1.5 | 低 |
| 7 | 导出线程不可取消 | ✅ 已解决 | v1.1.4 | 低 |
| 8 | CSV 导出 group_ids_filter 传递 | ✅ 已解决 | v1.1.4 | 低 |
| 9 | 多 QQ 号切换后数据不更新 | ✅ 已解决 | v1.1.4 | 低 |
| 10 | 大数据库加载报错 | ✅ 已解决 | v1.1.4 | 低 |
| 11 | TXT 导出格式不可读 | ✅ 已解决 | v1.1.4 | 低 |
| 12 | SQLCipher 索引创建过慢 | ✅ 已解决 | v1.1.4 | 高（见回归风险） |
| 13 | 索引方案回退 → MessageStore + 进度反馈 | ✅ 已解决 | v1.1.4 | 高（见回归风险） |
| 14 | BLOB 解析极端慢路径 (CPU 100%) | ✅ 已解决 | v1.1.5 | 低 |
| 15 | 空选导出全部 | ✅ 已解决 | v1.1.5 | 低 |
| 16 | UID 映射表查找策略不足 | ✅ 已解决 | v1.1.5 | 低 |
| 17 | CSV 中文乱码 (Excel) | ✅ 已解决 | v1.1.5 | 低 |

---

## 问题 1：分析命令死锁/永久加载 ✅ v1.1.2 已解决

### 现象
- 进入群聊分析 → 群列表正常显示 → 点击某个群 → "正在分析群聊数据..." 永不返回
- 数据量无论大小（6 条 / 数万条）均复现

### 根因
**`load_store_if_needed` 在持有 Mutex 锁的情况下执行昂贵的数据库操作**（10-30 秒），导致：
1. `start_export` 在 `std::thread::spawn` 中持有 `store_mutex` 执行 DB 加载
2. `analyze_group` 在 `spawn_blocking` 中调用 `load_store_if_needed` 尝试获取同一把锁
3. 导出线程持锁 → 分析线程阻塞 → "永久加载"

此外，同步 `fn` 命令在 Tauri 主线程执行，阻塞 UI 渲染。v1.1.1 改为 `async fn + tokio::spawn_blocking` 后不再卡死，但仍因锁竞争不返回。

### 修复历程
- **v1.1.1**：同步命令 → 异步命令（`spawn_blocking`），解决 UI 冻结但未解决锁竞争
- **v1.1.2**：`load_store_if_needed` 改为检查缓存 → 释放锁 → 执行 DB → double-check 后存储
- **v1.1.3**：UID 映射列检测改进（间接修复，映射失败导致分析数据为空）
- **v1.1.4**：
  - `load_store_if_needed` 返回 `Result`，失败时立即报错而非静默回退到 SQL 慢路径
  - `msg_store` 改为 `HashMap<String, Arc<MessageStore>>` 按 db_path 缓存，支持多账号
  - 无 group_id/peer_id 时用 `GROUP BY` 列表查询，不加载 MessageStore

### 验证记录
- ✅ 单群分析：6 条消息 < 1 秒返回
- ✅ 多群切换：每次点击即时返回（缓存命中）
- ✅ 大数据库 (190MB)：首次加载 30-60 秒（有进度反馈），后续秒开
- ✅ 两个 QQ 号切换：各自独立缓存，数据不混淆

### 涉及代码
| 文件 | 关键函数 | 作用 |
|------|----------|------|
| `src-tauri/src/commands.rs` | `load_store_if_needed` | 缓存检查 + DB 加载 |
| `src-tauri/src/commands.rs` | `analyze_group` / `analyze_private` | 异步命令入口 |
| `src-tauri/src/export_chat.rs` | `MessageStore::load` | `SELECT *` 全表扫描 + HashMap 分组 |

---

## 问题 2：导出缓慢 + 群聊无内容 + UID 显示原始 ID ✅ v1.1.2 ~ v1.1.5 已解决

### 现象
- 导出 TXT 需等待很长时间，进度日志不推送
- 群聊 TXT 文件有文件头但无消息内容；私聊正常
- 发送者显示为原始 UID（如 `u_-PBswiplK-7J7bmaQLA-mA`）

### 根因
**三个独立问题叠加：**

1. **UID 映射失败**：`load_uid_map` 列检测逻辑将序号列（值 "1", "2"...）误认为 QQ 号列
   - 原逻辑：找第一个非数字列 = UID，第一个数字列 = QQ 号
   - 实际列布局：`48901`(序号)、`48902`(UID)、`48912`(空)、`1002`(QQ号)
   - 第一列 `48901` 的值是纯数字序号，被误判为 QQ 号

2. **群聊无内容**：`extract_text` 只扫描以 Han 字符开头的文本 run，英文/数字开头的消息被跳过 → `classify_by_ascii` 返回 `[其他]`（早期版本不提取文本）

3. **导出无进度**：store 快路径中没有调用 `log()` 推送进度

### 子问题与修复对应
| 子问题 | 根因 | 修复版本 |
|--------|------|----------|
| UID 显示原始 ID | `load_uid_map` 列检测错误（猜测 A 正确） | v1.1.3 → v1.1.5 |
| 群聊无内容 | `extract_text` 跳过 ASCII 开头消息（猜测 B 正确） | v1.1.3 |
| 导出无进度 | store 快路径未调用 `log()` | v1.1.2 |
| 猜测 C（列索引错位） | ❌ 已排除：数据库分析确认列索引正确 | — |

### 修复历程
- **v1.1.2**：扩展映射表候选名 (6→12)，增加模糊搜索关键词；添加导出进度日志
- **v1.1.3**：
  - `extract_text` 新增三阶段扫描：UTF-8 整体 → Han-initiated → 通用 UTF-8 text run
  - `load_uid_map` 列检测：UID 列找 `u_` 前缀，QQ 列找 5-12 位纯数字
- **v1.1.4**：TXT 导出格式重构（时间戳 + 日期分组 + 群名头）
- **v1.1.5**：
  - `load_uid_map` 重写为 4 级策略：已知列名直查 → 候选表自动检测 → 模糊表名搜索 → 全表暴力扫描
  - `extract_text` 新增操作预算 (`budget = blob_size × 50`)，防止极端慢路径
  - 空选不导出（前端始终传递选中 ID 列表）

### 验证记录
- ✅ UID 映射：1228 条映射全部正确加载，发送者显示为 QQ 号
- ✅ 群聊导出：163548 条消息，英文/中文/混合内容均正确提取
- ✅ 进度反馈：每个群开始时显示消息数，每 1000 条报告进度
- ✅ 空选行为：不选择任何群/私聊 → 点击导出 → 不导出任何内容

### 涉及代码
| 文件 | 关键函数 | 作用 |
|------|----------|------|
| `src-tauri/src/export_chat.rs` | `load_uid_map` | 4 级 UID 映射策略 |
| `src-tauri/src/message_parser.rs` | `extract_text` | BLOB 文本提取（含操作预算） |
| `src-tauri/src/export_chat.rs` | `decrypt_and_export` | TXT 导出主逻辑 |
| `src/pages/ExportPage.tsx` | `buildParams` | 前端参数构建（始终传递选中 ID） |

---

## 问题 3：导出进度不反馈 ✅ v1.1.2 ~ v1.1.4 已解决

### 现象
- 导出开始后进度日志区域无消息推送，只有静态"正在导出..."

### 根因
store 快路径的群聊/私聊导出循环中没有调用 `log()` 推送进度。SQL 回退路径有 `log("info", ...)` 但快路径遗漏。

### 修复
- **v1.1.2**：store 快路径每个会话开始时显示消息数和群名称，每 5000 条报告进度
- **v1.1.4**：
  - `MessageStore::load_with_progress()` 每 5 万条报告加载进度
  - `start_export` 检测到无缓存时显示"正在加载数据库..."
  - I/O 写入错误不再用 `.ok()` 静默吞掉，记录错误日志

### 验证记录
- ✅ 首次加载：日志显示 "群消息进度: 50000 条 (15s)..." → "加载完成: 164 个群, 12 个私聊"
- ✅ 导出过程：每个群开始时显示群名 + 消息数，每 1000 条报告
- ✅ 导出完成：显示总消息数、群聊数、私聊数

---

## 问题 4：BLOB 解析含 Protobuf 垃圾 / JSON 元数据 ✅ v1.1.1 ~ v1.1.5 已解决

### 根因
QQ 消息 BLOB (列 `40800`) 包含 Protobuf 编码的二进制数据。早期解析器将 varint 字节解码为 CJK 扩展区字符（0x3400-0x4DBF），产生大量"生僻字"垃圾输出。

### 修复
- **v1.1.1**：
  - `is_common_han` 收敛到基本汉字区 0x4E00-0x9FA5（排除扩展区）
  - `is_valid_chat_text` 纯净度过滤器（>60% common 字符才保留）
  - 大 BLOB (>64KB) 快通：只扫前 8KB 媒体签名
  - `extract_prompt` 提取 JSON `"prompt":"..."` 字段
- **v1.1.3**：`classify_by_ascii` 改为先尝试提取可读文本再回退到 `[其他]`
- **v1.1.5**：
  - `extract_text` 新增操作预算 (`budget = blob_size × 50`)，防止二进制 BLOB 导致的极端慢路径
  - 超出预算自动回退到 `classify_by_ascii` 快速分类
  - 移除线程超时机制（会导致僵尸线程吃满 CPU 且不释放），改为纯内联预算控制

### 已知权衡
`is_common_han` 仅覆盖基本汉字区 (0x4E00-0x9FA5)，不含 CJK 扩展区 A/B 和兼容汉字区。部分繁体字、罕用字会被归入"非通用字符"，纯净度比率可能低于 60% 阈值而被误判为 Protobuf 噪音。扩展区字节恰好是 Protobuf varint 高频伪装区，放开范围会导致大量垃圾输出。如果用户反馈繁体内容被吞，可考虑将阈值从 60% 适当下调。

### 验证记录
- ✅ 纯文本消息：中文、英文、emoji、混合内容正确提取
- ✅ 图片/视频/语音消息：正确识别并显示占位符
- ✅ JSON 广告消息：提取 `prompt` 字段，清理 `[小程序]` 前缀
- ✅ 二进制 BLOB：操作预算触发后快速回退，CPU 不再拉满

---

## 问题 5：`extract_text` 文本提取不全 ✅ v1.1.3 已解决

### 根因
原 `extract_text` 只扫描以 common Han 字符开头的文本 run。消息 BLOB 中以 ASCII 字符开头（英文、数字、emoji）或文本在中间偏移位置时，扫描器跳过。`classify_by_ascii` 不提取文本内容，直接返回 `[其他]`。

### 修复
三阶段扫描：
1. 将整个 BLOB 尝试 UTF-8 解码（纯文本消息快路径）
2. Han-initiated 扫描（中文开头的消息）
3. 通用 UTF-8 text run 扫描（ASCII 开头或混合内容）
4. `classify_by_ascii` 先尝试提取可读文本，再回退到 `[其他]`

### 验证记录
- ✅ 英文消息："Hello World" 正确提取
- ✅ 数字消息："12345" 正确提取
- ✅ emoji 开头消息：正确提取后续文本
- ✅ 混合消息："abc中文123" 正确提取

---

## 问题 6：UID 映射列检测错误 ✅ v1.1.3 ~ v1.1.5 已解决

### 根因
`nt_uid_mapping_table` 列布局为 `48901`(序号)、`48902`(UID)、`48912`(空)、`1002`(QQ号)。原逻辑找第一个非数字列作为 UID、第一个数字列作为 QQ 号，但第一列 `48901` 的值是纯数字序号，被误判为 QQ 号。

### 修复
- **v1.1.3**：UID 列查找以 `u_` 开头或长度 >5 的非数字字符串；QQ 列查找 5-12 位纯数字
- **v1.1.5**：`load_uid_map` 重写为 4 级策略：
  1. 已知列名直查（`48901`/`40020`）
  2. 候选表自动检测（读取前 5 行样本，识别 `u_` 前缀 UID + 5-12 位数字 QQ）
  3. 模糊表名搜索（uid/mapping/friend/contact/buddy）
  4. 全表暴力扫描（跳过 msg 表）

### 验证记录
- ✅ `nt_uid_mapping_table`：1228 条映射正确加载
- ✅ 自动检测：5 行样本中识别出 `u_` 前缀列和数字列
- ✅ 暴力扫描：候选表均不存在时，扫描所有表找到映射表

---

## 问题 7：导出线程不可取消 ✅ v1.1.4 已解决

### 根因
用户点击"取消导出"时，前端停止轮询进度日志，但 Rust 后台线程仍继续执行，浪费 CPU 和 I/O 资源。

### 修复
- `AppState` 新增 `cancel_flag: Arc<AtomicBool>`
- 新增 `cancel_export` Tauri 命令，设置取消标志
- `start_export` / `decrypt_and_export` / `export_csv` 每个群/联系人处理前检查标志
- 前端 `api.cancelExport()` 调用后端，同时停止轮询
- 取消后返回已完成的导出结果，不丢失已写入的文件

### 验证记录
- ✅ 导出中途取消：立即停止，显示已完成部分的统计
- ✅ 取消后文件完整：已写入的 TXT/CSV 文件不损坏

---

## 问题 8：CSV 导出 group_ids_filter 传递 ✅ v1.1.4 已解决

### 背景
验证 `group_ids_filter` 在全链路传递中无遗漏。本质上不是 bug，是代码审查确认。

### 结论
✅ 全链路检查确认正确：
- `ExportPage.tsx` → `buildParams()` → `api.exportCsv({group_ids})`
- `commands.rs` → `ExportParams.group_ids` → `export_csv(group_ids_filter)`
- 两端都正确使用过滤器构建 `HashSet<&str>` 进行群聊选择

---

## 问题 9：多 QQ 号切换后数据不更新 ✅ v1.1.4 已解决

### 根因
`MessageStore` 全局缓存 `msg_store: Arc<Mutex<Option<Arc<MessageStore>>>>` 只检查是否存在缓存，不检查缓存的数据库路径是否与当前请求一致。切换 QQ 号时 `load_store_if_needed` 直接返回已有缓存。

### 修复
`msg_store` 改为 `Arc<Mutex<HashMap<String, Arc<MessageStore>>>>`，以 `db_path` 为键。`load_store_if_needed` 和 `start_export` 均按 `db_path` 查找缓存。

### 验证记录
- ✅ 两个 QQ 号各自独立缓存
- ✅ 切换后数据正确更新

---

## 问题 10：大数据库加载报错而非静默挂死 ✅ v1.1.4 已解决

### 根因
`load_store_if_needed` 返回 `Option`，DB 打开或加载失败时静默返回 `None`，调用方回退到 SQL 慢路径。对 190MB 数据库，无索引的 SQL WHERE 扫描需要数十分钟，用户以为卡死。

### 修复
- `load_store_if_needed` 返回 `Result<Arc<MessageStore>, String>`，失败时明确报错
- `analyze_group` / `analyze_private` / `export_csv` 加载失败时立即返回错误
- `MessageStore::load` 增加分步进度日志（每 5 万条报告）
- 新增 `clear_msg_store` 命令，用户可强制清除缓存重新加载

### 验证记录
- ✅ 数据库打开失败：前端显示错误信息
- ✅ 加载过程：日志显示 "群消息进度: 50000 条 (15s)..."
- ✅ 强制清除缓存：`clear_msg_store` 正常工作

---

## 问题 11：TXT 导出格式不可读 ✅ v1.1.4 已解决

### 根因
导出循环中 `writeln!` 只写了原始文本内容，没有对 `msg_id`（时间戳）排序、没有将纳秒时间戳转换为可读时间、没有按日期分组逻辑。

### 修复
- 消息按 `msg_id`（时间戳）排序
- 文件头：群名/群号、消息总数、时间范围
- 每条消息：`[时:分:秒] 发送者：\n消息内容`，空行分隔
- 按日期分组：`———— 2024-01-15 星期一 ————`

### 验证记录
- ✅ 群聊导出：时间顺序正确，日期分组正确
- ✅ 私聊导出：格式与群聊一致
- ✅ 发送者名称：显示 QQ 号（UID 映射后）

---

## 问题 12 & 13：SQLCipher 索引方案 → 回退到 MessageStore ⚠️ v1.1.4 已解决（高回归风险）

### 背景
问题 12 尝试通过 `CREATE INDEX` 优化查询性能，问题 13 因索引创建过慢而回退。

### 问题 12：索引方案
- `open_db_at_path` 自动 `CREATE INDEX IF NOT EXISTS` 在 `40021` 和 `40020` 列
- 索引持久化在 `%TEMP%/qqflow_cache/` 磁盘缓存中
- 列出群/联系人用 `GROUP BY`（不加载 BLOB），详情/导出用 `WHERE` + 索引

### 问题 13：索引创建过慢 → 回退
- **现象**：190MB SQLCipher DB 上 `CREATE INDEX` 耗时 240 秒（4 分钟）
- **根因**：索引需全表扫描 + B-tree 构建 + 逐页 SQLCipher 加解密（48000 页 × 5ms/页）
- **回退方案**：
  - 移除自动索引创建
  - 恢复 MessageStore 全量加载 (`SELECT *` → HashMap)，实测只需 30-60 秒
  - `load_with_progress()` 每 5 万条报告进度
  - 列表查询保持 `GROUP BY`（不触发加载）

### ⚠️ 回归风险

| 风险 | 说明 | 缓解措施 |
|------|------|----------|
| 首次加载耗时 | MessageStore 全量扫描 16 万条 BLOB 需 30-60 秒，期间用户无法操作 | 进度日志每 5 万条报告，前端轮询显示 |
| 内存占用 | 16 万条消息全部加载到内存，190MB DB 对应约 200-400MB 内存 | 按 db_path 缓存，不同 QQ 号独立 |
| 无索引查询 | 如果 MessageStore 加载失败回退到 SQL，无索引的 WHERE 扫描极慢 | v1.1.4 已改为失败时报错而非回退 |
| 缓存一致性 | 修改数据库后缓存不会自动更新 | `clear_msg_store` 命令可手动清除 |

### 验证记录
- ✅ 首次加载：163548 条消息，~35s 完成（有进度日志）
- ✅ 缓存命中（第二次访问）：750ms 即时返回
- ✅ 进度日志：前端显示 "群消息进度: 50000 条 (15s)..."
- ✅ 加载失败：显示错误信息，不卡死

---

## 问题 14：BLOB 解析极端慢路径 (CPU 100%) ✅ v1.1.5 已解决

### 现象
导出过程中 CPU 占用率拉满，完成后不下降，必须关闭程序。

### 根因
二进制 BLOB（Protobuf 编码）被 `extract_text` 逐字节扫描，某些 BLOB 的 UTF-8 解码尝试产生大量无效循环。线程超时机制 (`std::thread::spawn + recv_timeout`) 会导致僵尸线程——超时后主线程返回，但工作线程仍在无限循环，吃满 CPU。

### 修复
- 移除线程超时机制（会导致僵尸线程）
- 在 `extract_text` 的两个扫描循环中添加操作计数器：
  ```rust
  let budget = n.saturating_mul(50);
  let mut ops = 0usize;
  while i < n {
      ops += 1;
      if ops > budget { return classify_by_ascii(blob); }
      // ... existing logic
  }
  ```
- 超出预算自动回退到 `classify_by_ascii` 快速分类
- 导出进度日志间隔从 5000 条降至 1000 条

### 验证记录
- ✅ 大量二进制 BLOB：CPU 不再拉满
- ✅ 导出完成后：CPU 正常回落
- ✅ 文本提取质量：预算内正常提取，超预算回退到 ASCII 分类

---

## 问题 15：空选导出全部 ✅ v1.1.5 已解决

### 现象
导出页面不选择任何群/私聊，点击"开始导出"后仍然导出全部内容。

### 根因
- 后端：`filter_set` 为 `None` 时默认导出全部
- 前端：`buildParams()` 在无选择时不发送 `group_ids`/`peer_ids` 字段，后端收到 `None`

### 修复
- **后端**：`filter_set` 为 `None` 时返回空 Vec（不导出）
  ```rust
  let group_ids: Vec<&String> = match &filter_set {
      None => Vec::new(),
      Some(f) => store.group_msgs.keys()
          .filter(|gid| f.contains(gid.as_str()))
          .collect(),
  };
  ```
- **前端**：`buildParams()` 始终传递选中 ID（含空列表）
  ```typescript
  p.group_ids = Array.from(selectedGroups)
  p.peer_ids = Array.from(selectedContacts)
  ```

### 验证记录
- ✅ 什么都不选 → 点击导出 → 不导出任何内容
- ✅ 选择部分群聊 → 只导出选中群
- ✅ 按钮显示实际选中数量

---

## 问题 16：UID 映射表查找策略不足 ✅ v1.1.5 已解决

### 根因
`load_uid_map` 依赖候选表名匹配，部分 QQ 版本使用不同表名或列名。

### 修复
4 级策略：
1. **已知列名直查**：`nt_uid_mapping_table` 的 `48901`/`40020` 列
2. **候选表自动检测**：12 个候选表名，读取前 5 行样本，识别 `u_` 前缀 UID + 5-12 位数字 QQ
3. **模糊表名搜索**：匹配 uid/mapping/friend/contact/buddy/troop/recent 关键词
4. **全表暴力扫描**：跳过 msg 表，扫描所有剩余表

### 验证记录
- ✅ `nt_uid_mapping_table`：1228 条映射正确加载
- ✅ 候选表扩展：新增 `recent_contact_table`、`nt_recent_contact_table` 等
- ✅ 自动检测逻辑：支持 String 和 i64 两种存储类型

---

## 问题 17：CSV 中文乱码 (Excel) ✅ v1.1.5 已解决

### 根因
CSV 文件使用 UTF-8 编码但没有 BOM 头，Excel/WPS 双击打开时按系统默认编码（GBK）解析，中文显示为乱码。

### 修复
所有 CSV 导出文件开头写入 UTF-8 BOM (`\xEF\xBB\xBF`)：
```rust
let mut file = std::fs::File::create(&csv_path).map_err(|e| e.to_string())?;
use std::io::Write;
file.write_all(&[0xEF, 0xBB, 0xBF]).map_err(|e| e.to_string())?;
let mut wtr = csv::Writer::from_writer(file);
```

### 验证记录
- ✅ Excel 双击打开：中文正常显示
- ✅ WPS 双击打开：中文正常显示
- ✅ 4 个 CSV 导出路径均已添加 BOM

---

## 架构背景

### 数据流
```
QQ 原始 .db 文件
  → std::io::copy (跳过 1024B 头部)
  → 磁盘缓存 (%TEMP%/qqflow_cache/<hash>)
  → rusqlite + SQLCipher 解密
  → MessageStore::load: SELECT * → HashMap<group_id, Vec<Msg>>
  → analyze / export (纯内存操作，不走 SQL)
```

### MessageStore 结构
```rust
pub struct MessageStore {
    pub group_msgs: HashMap<String, Vec<GroupMsg>>,  // 群号 → 消息列表
    pub c2c_msgs:   HashMap<String, Vec<C2cMsg>>,    // 对端UID → 消息列表
    pub uid_map:    HashMap<String, String>,           // UID → QQ号
    pub group_names: HashMap<String, String>,          // 群号 → 群名
}
```

### 分析函数调用链
```
前端 await api.analyzeGroup(params)
  → Tauri IPC
  → analyze_group (commands.rs, async)
    → spawn_blocking:
      → load_store_if_needed (加载/获取缓存)
      → analyze_group_detail (analysis.rs)
        → 快路径: store.group_msgs.get(group_id) → 遍历 Vec<GroupMsg>
      → 序列化为 JSON → 返回给前端
```

---

## 数据库参考信息

### 数据库结构 (nt_msg.db, 191MB)
| 表名 | 行数 | 说明 |
|------|------|------|
| `group_msg_table` | 163,548 | 群消息 |
| `c2c_msg_table` | 1,108 | 私聊消息 |
| `nt_uid_mapping_table` | 1,228 | UID 映射 |

### 关键列名
| 列名 | 类型 | 说明 |
|------|------|------|
| `40001` | INTEGER | 消息ID/纳秒时间戳 |
| `40020` | TEXT | 发送者UID |
| `40021` | TEXT | 群号 |
| `40093` | TEXT | 发送者昵称 |
| `40800` | BLOB | 消息内容 |

### UID 映射表列名
| 列名 | 类型 | 说明 |
|------|------|------|
| `48901` | INTEGER | 序号 |
| `48902` | TEXT | UID（如 `u_KnYbF-ogjSc09OqJP2_-lA`）|
| `1002` | INTEGER | QQ 号（如 `2919505399`）|

---

## 已知未解决问题 / 已知限制

### 1. 首次加载耗时
- 190MB 数据库首次 MessageStore 加载需 30-60 秒
- 有进度日志反馈，但用户仍需等待
- **缓解**：加载完成后缓存常驻，后续操作秒开

### 2. CJK 扩展区字符被过滤
- `is_common_han` 仅覆盖 0x4E00-0x9FA5，不含扩展区 A/B 和兼容汉字区
- 繁体字、罕用字可能被误判为 Protobuf 噪音
- **缓解**：纯净度阈值从 80% 降至 60%，减少误判

### 3. 密钥提取依赖 Windows Debug API
- 仅支持 Windows 10/11
- 需要 QQ 进程正在运行
- 部分安全软件可能拦截调试注入

### 4. 无自动更新机制
- 用户需手动下载新版本安装
- **建议**：后续可集成 Tauri updater 插件

### 5. 多 DB 文件未自动关联
- QQ NT 可能有多个 .db 文件（消息、联系人、群信息等）
- 当前需用户手动选择包含消息的 DB
- **建议**：后续可自动扫描并关联相关 DB

---

## 版本更新记录

### v1.1.5 (2026-05-24) — 首个公开版本

开发过程中经历了多轮迭代（内部版本 v1.1.1 ~ v1.1.5），以下为合并后的关键变更：

**架构**
- MessageStore 内存缓存：首次 `SELECT *` 全表扫描 → `HashMap`，后续查询 O(1)
- 异步命令架构：Tauri async + `tokio::spawn_blocking`，不阻塞 UI
- `load_store_if_needed` 死锁修复：检查缓存 → 释放锁 → DB 操作 → double-check
- MessageStore 按 `db_path` 独立缓存，支持多 QQ 号切换

**解析器**
- `extract_text` 三阶段扫描：UTF-8 整体 → Han-initiated → 通用 UTF-8 text run
- 操作预算 (`budget = blob_size × 50`)，防止极端慢路径
- `is_common_han` CJK 基本汉字区 (0x4E00-0x9FA5) + `is_valid_chat_text` 纯净度过滤

**UID 映射**
- 4 级策略：已知列名直查 → 候选表自动检测 → 模糊表名搜索 → 全表暴力扫描
- 候选表 12+，模糊搜索匹配 uid/mapping/friend/contact/buddy/troop/recent

**导出**
- TXT 格式：时间戳排序 + 日期分组 + 群名头
- CSV 格式：UTF-8 BOM，Excel/WPS 正确显示中文
- 空选不导出，用户必须显式勾选
- 导出线程可取消 (AtomicBool)
- 进度日志实时推送

**其他**
- 多 QQ 号密钥独立存储 (JSON 格式)
- 大数据库加载失败时报错而非静默回退
- `clear_msg_store` 命令可强制清除缓存
