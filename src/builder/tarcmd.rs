use std::fs::{create_dir_all, set_permissions, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use unshare::{Command, Stdio};

use container::mount::{bind_mount, unmount};
use config::builders::TarInfo;
use config::builders::TarInstallInfo;
use super::context::Context;
use super::download::download_file;
use super::commands::generic::run_command_at;
use file_util::{read_visible_entries, create_dir};
use path_util::ToRelative;


pub fn unpack_file(_ctx: &mut Context, src: &Path, tgt: &Path,
    includes: &[&Path], excludes: &[&Path])
    -> Result<(), String>
{
    info!("Unpacking {} -> {}", src.display(), tgt.display());
    let mut cmd = Command::new("/vagga/bin/busybox");
    cmd.stdin(Stdio::null())
        .arg("tar")
        .arg("-x")
        .arg("-f").arg(src)
        .arg("-C").arg(tgt);
    for i in includes.iter() {
        cmd.arg(i);
    }
    for i in excludes.iter() {
        cmd.arg("--exclude").arg(i);
    }

    match src.extension().and_then(|x| x.to_str()) {
        Some("gz")|Some("tgz") => { cmd.arg("-z"); }
        Some("bz")|Some("tbz") => { cmd.arg("-j"); }
        Some("xz")|Some("txz") => { cmd.arg("-J"); }
        _ => {}
    };
    info!("Running: {:?}", cmd);
    match cmd.status() {
        Ok(st) if st.success() => Ok(()),
        Ok(val) => Err(format!("Tar exited with status: {}", val)),
        Err(e) => Err(format!("Error running tar: {}", e)),
    }
}

pub fn tar_command(ctx: &mut Context, tar: &TarInfo) -> Result<(), String>
{
    let fpath = PathBuf::from("/vagga/root").join(tar.path.rel());
    let filename = if tar.url.starts_with(".") {
        PathBuf::from("/work").join(&tar.url)
    } else {
        try!(download_file(ctx, &tar.url))
    };
    // TODO(tailhook) check sha256 sum
    if &Path::new(&tar.subdir) == &Path::new(".") {
        try!(unpack_file(ctx, &filename, &fpath, &[], &[]));
    } else {
        let tmppath = PathBuf::from("/vagga/root/tmp")
            .join(filename.file_name().unwrap());
        let tmpsub = tmppath.join(&tar.subdir);
        try_msg!(create_dir(&tmpsub, true), "Error making dir: {err}");
        if !fpath.exists() {
            try_msg!(create_dir(&fpath, true), "Error making dir: {err}");
        }
        try!(bind_mount(&fpath, &tmpsub));
        let res = if tar.subdir.as_path() == Path::new("") {
            unpack_file(ctx, &filename, &tmppath, &[], &[])
        } else {
            unpack_file(ctx, &filename, &tmppath,
                &[&tar.subdir.clone()], &[])
        };
        try!(unmount(&tmpsub));
        try!(res);
    }
    Ok(())
}

pub fn tar_install(ctx: &mut Context, tar: &TarInstallInfo)
    -> Result<(), String>
{
    let filename = if tar.url.starts_with(".") {
        PathBuf::from("/work").join(&tar.url)
    } else {
        try!(download_file(ctx, &tar.url))
    };
    // TODO(tailhook) check sha256 sum
    let tmppath = PathBuf::from("/vagga/root/tmp")
        .join(filename.file_name().unwrap());
    try!(create_dir_all(&tmppath)
         .map_err(|e| format!("Error making dir: {}", e)));
    try!(set_permissions(&tmppath, Permissions::from_mode(0o755))
         .map_err(|e| format!("Error setting permissions: {}", e)));
    try!(unpack_file(ctx, &filename, &tmppath, &[], &[]));
    let workdir = if let Some(ref subpath) = tar.subdir {
        tmppath.join(subpath)
    } else {
        let items = try!(read_visible_entries(&tmppath)
            .map_err(|e| format!("Error reading dir: {}", e)));
        if items.len() != 1 {
            if items.len() == 0 {
                return Err("Tar archive was empty".to_string());
            } else {
                return Err("Multiple directories was unpacked. \
                    If thats expected use `subdir: \".\"` or any \
                    other directory".to_string());
            }
        }
        items.into_iter().next().unwrap()
    };
    let workdir = PathBuf::from("/").join(
        workdir.rel_to(&Path::new("/vagga/root")).unwrap());
    return run_command_at(ctx, &[
        "/bin/sh".to_string(),
        "-exc".to_string(),
        tar.script.to_string()],
        &workdir);
}
