use std::ffi::OsStr;
use std::process::Command;

use crate::error::Error;

pub fn git<I, S>(args: I) -> Result<String, Error>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new("git")
        .args(args)
        .env("LC_ALL", "C.UTF-8")
        .output()
        .map_err(|e| format!("Failed to execute git: {}", e))?;

    if output.status.success() {
       String::from_utf8(output.stdout)
           .map(|s| s.trim().to_string())
           .map_err(|e| Error(format!("Invalid UTF-8: {}", e)))
   } else {
       String::from_utf8(output.stderr)
           .map_err(|e| Error(format!("Invalid UTF-8 in stderr: {}", e)))
   }
}

pub fn tf<I, S>(args: I) -> Result<String, Error>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new("tf")
        .args(args)
        .env("LC_ALL", "C.UTF-8")
        .env("TF_ADDITIONAL_JAVA_ARGS", "-Dfile.encoding=UTF-8 -Duser.language=en -Duser.country=US")
        .output()
        .map_err(|e| format!("Failed to execute tf: {}", e))?;

    if output.status.success() {
        String::from_utf8(output.stdout)
            .map(|s| s.trim().to_string())
            .map_err(|e| Error(format!("Invalid UTF-8: {}", e)))
    } else {
        String::from_utf8(output.stderr)
            .map_err(|e| Error(format!("Invalid UTF-8 in stderr: {}", e)))
    }
}
