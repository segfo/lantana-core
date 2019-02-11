#![allow(dead_code)]
pub const CONFIG_DIR:&'static str="config";
pub const PLUGIN_DIR:&'static str="plugins";
pub const LOG_CONF:&'static str="log4rs.toml";
pub const GLOBAL_CONF:&'static str ="lantana.toml";
pub const BROKEN_CONF_BACKUP_PREFIX:&'static str = "Broken_%Y-%m-%d_%H%M%S_";
pub const CONF_BACKUPPED:&'static str="The configuration file has been backed up.";
pub const CAUSE:&'static str = "Cause";
pub const BACKUP_WRITTEN:&'static str = "Backup written";
pub const PLUGIN_ENTRY_POINT:&'static str = "iris_entry";
pub const LONG_WAIT_THREAD_SECS:u32=30;
pub const ONE_SEC:u64=990;
