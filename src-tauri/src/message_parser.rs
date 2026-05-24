/// Message BLOB parser for QQ NT messages.
/// Parses the binary BLOB from column 40800 to extract text and classify message type.
/// Uses a "text purity" filter to reject Protobuf varint noise that masquerades as
/// rare CJK characters in Unicode extension blocks.

#[derive(Debug, Clone, serde::Serialize)]
pub struct ParsedMessage {
    pub msg_type: String,
    pub content: String,
}

// ── Purity filter for extracted text ──

/// Check if a character is in the common Han (CJK Unified Ideographs) range.
/// Deliberately excludes extension blocks (0x3400-0x4DBF, 0x20000-0x2A6DF, etc.)
/// because Protobuf varint length markers frequently decode to code points in
/// those ranges, producing garbled "rare character" output.
fn is_common_han(cp: u32) -> bool {
    (0x4E00..=0x9FA5).contains(&cp)
}

/// Determine if extracted text looks like real chat content rather than
/// Protobuf binary garbage that happened to decode as Unicode.
fn is_valid_chat_text(text: &str) -> bool {
    let char_count = text.chars().count();
    // Single-char "messages" are almost always noise from varint decoding
    if char_count < 2 {
        return false;
    }

    let mut common = 0u32;
    let mut suspect = 0u32;

    for c in text.chars() {
        let cp = c as u32;
        if is_common_han(cp) || c.is_ascii_alphanumeric() || c.is_ascii_punctuation() || c == ' ' {
            common += 1;
        } else if c.is_control() {
            // Control characters are always suspect (Protobuf noise)
            suspect += 1;
        }
        // Non-ASCII printable chars (emoji, symbols, etc.) are neither common nor suspect
        // — they're valid chat content but don't count toward the purity ratio
    }

    let total = common + suspect;
    if total == 0 {
        return false;
    }

    // Require >60% common characters — real chat text is dominated by
    // common Han, ASCII, and punctuation. Protobuf binary has high
    // entropy in the "suspect" range. Lowered from 80% to accommodate
    // emoji and special characters in real QQ messages.
    (common as f64) / (total as f64) > 0.60
}

// ── UTF-8 helpers ──

/// Collect a continuous UTF-8 text run starting at `start` in `blob`.
fn collect_text_run(blob: &[u8], start: usize) -> (String, usize) {
    let mut i = start;
    let n = blob.len();
    let mut run = String::new();

    while i < n {
        let b = blob[i];
        let cl = if b >= 0xF0 && i + 4 <= n {
            4
        } else if b >= 0xE0 && i + 3 <= n {
            3
        } else if b >= 0xC0 && i + 2 <= n {
            2
        } else if (32..=126).contains(&b) || b == 0x0A || b == 0x0D {
            1
        } else {
            break;
        };

        match std::str::from_utf8(&blob[i..i + cl]) {
            Ok(s) => {
                run.push_str(s);
                i += cl;
            }
            Err(_) => break,
        }
    }

    (run.trim().to_string(), i)
}

fn is_continuation(b: u8) -> bool {
    (0x80..=0xBF).contains(&b)
}

/// Decode a UTF-8 character starting at position `i` in `blob`.
fn decode_utf8_char(blob: &[u8], i: usize) -> Option<(u32, usize)> {
    let b = blob[i];
    let (ch_len, cp) = if b >= 0xF0 && i + 4 <= blob.len()
        && is_continuation(blob[i + 1]) && is_continuation(blob[i + 2]) && is_continuation(blob[i + 3])
    {
        let cp = ((b & 0x07) as u32) << 18
            | ((blob[i + 1] & 0x3F) as u32) << 12
            | ((blob[i + 2] & 0x3F) as u32) << 6
            | ((blob[i + 3] & 0x3F) as u32);
        (4, cp)
    } else if b >= 0xE0 && i + 3 <= blob.len()
        && is_continuation(blob[i + 1]) && is_continuation(blob[i + 2])
    {
        let cp = ((b & 0x0F) as u32) << 12
            | ((blob[i + 1] & 0x3F) as u32) << 6
            | ((blob[i + 2] & 0x3F) as u32);
        (3, cp)
    } else if b >= 0xC0 && i + 2 <= blob.len()
        && is_continuation(blob[i + 1])
    {
        let cp = ((b & 0x1F) as u32) << 6 | ((blob[i + 1] & 0x3F) as u32);
        (2, cp)
    } else {
        return None;
    };
    Some((cp, ch_len))
}

// ── Main extraction ──

/// Extract text from a QQ NT message BLOB (column 40800).
/// Returns (msg_type, content).
pub fn extract_text(blob: &[u8]) -> ParsedMessage {
    if blob.is_empty() {
        return ParsedMessage { msg_type: "text".to_string(), content: "[空]".to_string() };
    }

    // Fast path: large BLOBs (>64KB) are almost certainly media, not text.
    // Avoid expensive byte-by-byte scan — just check for media signatures.
    if blob.len() > 65536 {
        return classify_large_blob(blob);
    }

    // Try to extract as UTF-8 text first (handles plain text messages)
    if let Ok(text) = std::str::from_utf8(blob) {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            // Check for JSON-structured messages (mini-programs, shares, cards)
            if let Some(extracted) = extract_json_blob(trimmed) {
                return ParsedMessage { msg_type: "miniapp".to_string(), content: extracted };
            }
            if is_valid_chat_text(trimmed) {
                let content = extract_prompt(trimmed);
                if content.starts_with("你猜猜撤回了什么") {
                    return ParsedMessage { msg_type: "recall".to_string(), content: "[撤回了一条消息]".to_string() };
                }
                if content.contains("戳了搓") || content.contains("拍了拍")
                    || content.contains("撤回了一条") || content.contains("修改群名")
                {
                    return ParsedMessage { msg_type: "system".to_string(), content };
                }
                return ParsedMessage { msg_type: "text".to_string(), content };
            }
        }
    }

    // Scan for Han-character-initiated text runs (original approach)
    let n = blob.len();
    let budget = n.saturating_mul(50); // operation budget to prevent pathological slowness
    let mut texts: Vec<String> = Vec::new();
    let mut i = 0;
    let mut ops = 0usize;

    while i < n {
        ops += 1;
        if ops > budget { return classify_by_ascii(blob); }
        if let Some((cp, ch_len)) = decode_utf8_char(blob, i) {
            if is_common_han(cp) {
                let (text, new_i) = collect_text_run(blob, i);
                if text.len() >= 2 && is_valid_chat_text(&text) {
                    texts.push(text);
                }
                i = new_i;
                continue;
            }
            i += ch_len;
        } else {
            i += 1;
        }
    }

    if !texts.is_empty() {
        let content = texts.join(" ");
        let content = trim_json_suffix(&content);
        let content = extract_prompt(&content);
        // Check for JSON blob patterns in joined text runs
        if let Some(extracted) = extract_json_blob(&content) {
            return ParsedMessage { msg_type: "miniapp".to_string(), content: extracted };
        }
        if content.starts_with("你猜猜撤回了什么") {
            return ParsedMessage { msg_type: "recall".to_string(), content: "[撤回了一条消息]".to_string() };
        }
        if content.contains("戳了搓") || content.contains("拍了拍")
            || content.contains("撤回了一条") || content.contains("修改群名")
        {
            return ParsedMessage { msg_type: "system".to_string(), content };
        }
        return ParsedMessage { msg_type: "text".to_string(), content };
    }

    // Scan for any UTF-8 text runs (not just Han-initiated)
    // This handles messages that start with ASCII or mixed content
    let mut i = 0;
    let mut ops = 0usize;
    while i < n {
        ops += 1;
        if ops > budget { return classify_by_ascii(blob); }
        if let Some((_cp, ch_len)) = decode_utf8_char(blob, i) {
            if ch_len > 1 || (32..=126).contains(&blob[i]) {
                let (text, new_i) = collect_text_run(blob, i);
                if text.len() >= 3 {
                    if let Some(extracted) = extract_json_blob(&text) {
                        return ParsedMessage { msg_type: "miniapp".to_string(), content: extracted };
                    }
                    if is_valid_chat_text(&text) {
                        let content = extract_prompt(&text);
                        return ParsedMessage { msg_type: "text".to_string(), content };
                    }
                }
                i = new_i;
                continue;
            }
            i += ch_len;
        } else {
            i += 1;
        }
    }

    classify_by_ascii(blob)
}

/// Quick classification for large BLOBs (media).
fn classify_large_blob(blob: &[u8]) -> ParsedMessage {
    let sample = &blob[..blob.len().min(8192)];
    let ascii: String = sample.iter().filter(|&&b| (32..=126).contains(&b)).map(|&b| b as char).collect();
    if ascii.contains(".jpg") || ascii.contains(".png") || ascii.contains(".gif") || ascii.contains("gchatpic") {
        return ParsedMessage { msg_type: "image".to_string(), content: "[图片]".to_string() };
    }
    if ascii.contains(".amr") || ascii.contains(".silk") || ascii.contains(".ptt") {
        return ParsedMessage { msg_type: "voice".to_string(), content: "[语音]".to_string() };
    }
    if ascii.to_lowercase().contains("shortvideo") || ascii.contains(".mp4") {
        return ParsedMessage { msg_type: "video".to_string(), content: "[短视频]".to_string() };
    }
    ParsedMessage { msg_type: "other".to_string(), content: "[其他]".to_string() }
}

/// Classify message by scanning ASCII content in the BLOB.
fn classify_by_ascii(blob: &[u8]) -> ParsedMessage {
    let ascii_content: String = blob.iter()
        .filter(|&&b| (32..=126).contains(&b))
        .map(|&b| b as char).collect();

    if ascii_content.contains(".jpg") || ascii_content.contains(".png")
        || ascii_content.contains(".gif") || ascii_content.contains("gchatpic")
    {
        return ParsedMessage { msg_type: "image".to_string(), content: "[图片]".to_string() };
    }
    if ascii_content.contains(".amr") || ascii_content.contains(".silk") || ascii_content.contains(".ptt") {
        return ParsedMessage { msg_type: "voice".to_string(), content: "[语音]".to_string() };
    }
    if ascii_content.to_lowercase().contains("shortvideo") || ascii_content.contains(".mp4") {
        return ParsedMessage { msg_type: "video".to_string(), content: "[短视频]".to_string() };
    }
    // Try to extract readable text from the BLOB
    if let Ok(text) = std::str::from_utf8(blob) {
        let text = text.trim();
        if !text.is_empty() && is_valid_chat_text(text) {
            let content = extract_prompt(text);
            return ParsedMessage { msg_type: "text".to_string(), content };
        }
    }
    // For small BLOBs, try harder to find text
    if blob.len() < 500 {
        // Scan for any printable ASCII runs of 4+ characters
        let mut best_run = String::new();
        let mut current_run = String::new();
        for &b in blob {
            if (32..=126).contains(&b) {
                current_run.push(b as char);
            } else {
                if current_run.len() > best_run.len() {
                    best_run = current_run.clone();
                }
                current_run.clear();
            }
        }
        if current_run.len() > best_run.len() {
            best_run = current_run;
        }
        if best_run.len() >= 4 && is_valid_chat_text(&best_run) {
            return ParsedMessage { msg_type: "text".to_string(), content: best_run };
        }
    }
    ParsedMessage { msg_type: "other".to_string(), content: "[其他]".to_string() }
}

/// Extract the human-readable `prompt` field from QQ structured ad messages.
/// Input: `...发现异星生命！","meta":{...},"prompt":"发现异星生命！集结领取预约礼包",...`
/// Output: `发现异星生命！集结领取预约礼包`
fn extract_prompt(text: &str) -> String {
    if let Some(pos) = text.find("\"prompt\":\"") {
        let start = pos + 10;
        if let Some(rest) = text.get(start..) {
            let mut in_escape = false;
            for (i, c) in rest.char_indices() {
                if in_escape { in_escape = false; continue; }
                if c == '\\' { in_escape = true; continue; }
                if c == '"' {
                    let prompt = rest[..i].replace("\\\"", "\"").replace("\\n", "\n").replace("\\t", "\t");
                    if !prompt.is_empty() { return prompt; }
                    break;
                }
            }
        }
    }
    text.to_string()
}

/// Extract a single JSON string field value. Handles \" escapes.
fn extract_json_field(text: &str, field_name: &str) -> Option<String> {
    let search = format!("\"{}\":\"", field_name);
    let pos = text.find(&search)?;
    let after_key = &text[pos + search.len()..];
    let mut result = String::new();
    let mut in_escape = false;
    for c in after_key.chars() {
        if in_escape { result.push(c); in_escape = false; continue; }
        if c == '\\' { in_escape = true; continue; }
        if c == '"' { break; }
        result.push(c);
    }
    let cleaned = result.replace("\\/", "/").replace("\\n", " ").replace("\\t", " ").trim().to_string();
    if cleaned.is_empty() { None } else { Some(cleaned) }
}

/// Check if text looks like JSON-structured data (app share / mini-program / card).
fn is_json_blob(text: &str) -> bool {
    let total = text.chars().count() as f64;
    if total < 50.0 { return false; }
    let json_chars = text.chars().filter(|c| matches!(c, '{'|'}'|'"'|':'|'['|']')).count() as f64;
    // More than ~12% of chars are JSON syntax → structured message
    json_chars / total > 0.12
}

/// Try to extract human-readable content from a JSON-structured QQ message BLOB.
/// Returns Some(content) if the BLOB is JSON-structured, None if it should be treated as plain text.
fn extract_json_blob(text: &str) -> Option<String> {
    if !is_json_blob(text) { return None; }

    let mut parts: Vec<String> = Vec::new();

    // Try prompt field (most common for shares/mini-programs)
    if let Some(p) = extract_json_field(text, "prompt") {
        // Clean up QQ's [...] prefix: [小程序], [分享], [视频], etc.
        let cleaned = p.trim_start_matches(|c: char| c == '[' || c == ']' || c.is_whitespace()
            || matches!(c, '小'|'程'|'序'|'分'|'享'|'视'|'频'|'文'|'件'|'链'|'接'|'图'|'片'));
        if !cleaned.is_empty() { parts.push(cleaned.to_string()); }
    }
    // Try desc field (common in app shares)
    if let Some(d) = extract_json_field(text, "desc") {
        if !parts.iter().any(|p| p == &d) { parts.push(d); }
    }
    // Try title field
    if let Some(t) = extract_json_field(text, "title") {
        if !parts.iter().any(|p| p == &t) { parts.push(t); }
    }
    // Try nick field (sender nick in share)
    if let Some(n) = extract_json_field(text, "nick") {
        let nick_text = format!("来自: {}", n);
        if !parts.iter().any(|p| p.contains(&nick_text)) { parts.push(nick_text); }
    }

    if parts.is_empty() {
        // Couldn't extract meaningful fields, return generic label
        Some("[小程序/分享]".to_string())
    } else {
        Some(parts.join(" | "))
    }
}

/// Trim trailing JSON garbage from a text run.
/// Many QQ message BLOBs have readable text followed by JSON metadata.
fn trim_json_suffix(text: &str) -> String {
    // If the text contains ",\"appID\"" or similar JSON appendages, cut early
    // Find the last natural sentence-ending position before JSON starts
    if let Some(pos) = text.find(",\"appID\"") {
        return text[..pos].to_string();
    }
    if let Some(pos) = text.find(",\"appid\"") {
        return text[..pos].to_string();
    }
    // Find where JSON structure starts after readable text
    // Pattern: Chinese text ends, then "}," or "],"
    for pattern in &["\",\"appID\"", "\",\"appid\"", "\",\"meta\"", "\",\"config\""] {
        if let Some(pos) = text.find(pattern) {
            let trimmed = text[..pos + 1].to_string(); // include the closing quote
            if is_valid_chat_text(&trimmed) { return trimmed; }
        }
    }
    text.to_string()
}
