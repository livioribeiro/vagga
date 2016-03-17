use std::path::{Path, PathBuf};
use std::collections::BTreeMap;

pub fn find_cmd(cmd: &str, env: &BTreeMap<String, String>)
    -> Result<PathBuf, String>
{
    if cmd.contains("/") {
        return Ok(PathBuf::from(cmd));
    } else {
        if let Some(paths) = env.get(&"PATH".to_string()) {
            for dir in paths[..].split(':') {
                let path = Path::new(dir);
                if !path.is_absolute() {
                    warn!("All items in PATH must be absolute, not {}",
                          path.display());
                    continue;
                }
                let path = path.join(cmd);
                if path.exists() {
                    return Ok(path);
                }
            }
            return Err(format!("Command {} not found in {:?}",
                cmd, paths));
        } else {
            return Err(format!("Command {} is not absolute and no PATH set",
                cmd));
        }
    }
}
