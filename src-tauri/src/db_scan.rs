/// Database scanning module.
/// Finds all QQ NT chat databases on the local machine.

use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct DatabaseInfo {
    pub qq: String,
    pub path: String,
    pub size_mb: f64,
}

/// Scan for all QQ NT message databases.
/// Looks in %USERPROFILE%\Documents\Tencent Files
pub fn find_qq_databases() -> Vec<DatabaseInfo> {
    let mut results = Vec::new();

    let Ok(user_profile) = std::env::var("USERPROFILE") else {
        return results;
    };

    let base = PathBuf::from(&user_profile).join("Documents").join("Tencent Files");
    if !base.is_dir() {
        return results;
    }

    let Ok(entries) = std::fs::read_dir(&base) else {
        return results;
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Must be a QQ number (digits only)
        if !name_str.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }

        let db_path = entry
            .path()
            .join("nt_qq")
            .join("nt_db")
            .join("nt_msg.db");

        if db_path.is_file() {
            let size = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);
            results.push(DatabaseInfo {
                qq: name_str.to_string(),
                path: db_path.to_string_lossy().to_string(),
                size_mb: (size as f64) / 1024.0 / 1024.0,
            });
        }
    }

    // Also check global nt_qq directory
    let global_db = base.join("nt_qq").join("nt_db").join("nt_msg.db");
    if global_db.is_file() {
        let size = std::fs::metadata(&global_db).map(|m| m.len()).unwrap_or(0);
        results.push(DatabaseInfo {
            qq: "global".to_string(),
            path: global_db.to_string_lossy().to_string(),
            size_mb: (size as f64) / 1024.0 / 1024.0,
        });
    }

    results
}
