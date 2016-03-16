use std::path::{Path, PathBuf};
use std::fs::File;
use std::io::{Read, Write};

use regex::Regex;

use super::super::context::Context;
use super::super::packages;
use super::generic::{capture_command, run_command_at_env};
use builder::error::StepError;
use builder::distrib::Distribution;
use builder::commands::generic::{command, run};
use config::builders::{GemSettings, GemBundleInfo};
use process_util::capture_stdout;

const DEFAULT_GEM_EXE: &'static str = "/usr/bin/gem";
const BIN_DIR: &'static str = "/usr/local/bin";
const GEM_VERSION_WITH_NO_DOCUMENT_OPT: f32 = 2.0;
const VALID_TRUST_POLICIES: [&'static str; 3] = ["LowSecurity", "MediumSecurity", "HighSecurity"];


impl Default for GemSettings {
    fn default() -> Self {
        GemSettings {
            install_ruby: true,
            gem_exe: None,
            update_gem: true,
        }
    }
}

fn no_doc_args(ctx: &mut Context) -> Result<Vec<&'static str>, String> {
    if ctx.gem_settings.update_gem {
        Ok(vec!("--no-document"))
    } else {
        let version = try!(gem_version(ctx));
        if version < GEM_VERSION_WITH_NO_DOCUMENT_OPT {
            Ok(vec!("--no-rdoc", "--no-ri"))
        } else {
            Ok(vec!("--no-document"))
        }
    }
}

fn gem_version(ctx: &mut Context) -> Result<f32, String> {
    let gem_exe = ctx.gem_settings.gem_exe.clone()
        .unwrap_or(DEFAULT_GEM_EXE.to_owned());

    let args = [
        gem_exe,
        "--version".to_owned(),
    ];

    let gem_ver = try!(capture_command(ctx, &args, &[])
        .and_then(|x| String::from_utf8(x)
            .map_err(|e| format!("Error parsing gem version: {}", e)))
        .map_err(|e| format!("Error getting gem version: {}", e)));

    let re = Regex::new(r#"^(\d+?\.\d+?)\."#).expect("Invalid regex");
    let version = try!(re.captures(&gem_ver)
        .and_then(|cap| cap.at(1))
        .ok_or("Gem version was not found".to_owned()));

    version.parse::<f32>()
        .map_err(|e| format!("Erro parsing gem version: {}", e))
}

fn gem_cache_dir(ctx: &mut Context) -> Result<PathBuf, String> {
    let gem_exe = ctx.gem_settings.gem_exe.clone()
        .unwrap_or(DEFAULT_GEM_EXE.to_owned());

    let args = [
        gem_exe,
        "env".to_owned(),
        "gemdir".to_owned(),
    ];

    let gem_dir = try!(capture_command(ctx, &args, &[])
        .and_then(|x| String::from_utf8(x)
            .map_err(|e| format!("Error getting gem dir: {}", e))));

    Ok(Path::new(gem_dir.trim()).join("cache"))
}

fn requires_git(gemfile: &Path) -> Result<bool, String> {
    let gemfile = Path::new("/work").join(gemfile);

    let re = Regex::new(
        r#"(git .*? do)|(:(git|github|gist|bitbucket) =>)|(git_source\(.*?\))"#
    ).expect("Invalid regex");

    let gemfile_data = {
        let mut buf = String::new();
        try!(File::open(&gemfile)
            .and_then(|mut f| f.read_to_string(&mut buf))
            .map_err(|e| format!("Error reading Gemfile ({:?}): {}", &gemfile, e)));

        buf
    };

    Ok(re.is_match(&gemfile_data))
}

fn scan_features(settings: &GemSettings, info: Option<&GemBundleInfo>)
    -> Result<Vec<packages::Package>, String>
{
    let mut res = vec!();
    res.push(packages::BuildEssential);

    if settings.install_ruby {
        res.push(packages::Ruby);
        res.push(packages::RubyDev);
    }

    res.push(packages::Bundler);

    if let Some(info) = info {
        let git_required = try!(requires_git(&info.gemfile));
        if git_required {
            res.push(packages::Git);
        }
    }

    Ok(res)
}

pub fn install(distro: &mut Box<Distribution>,
    ctx: &mut Context, pkgs: &Vec<String>)
    -> Result<(), String>
{
    let features = try!(scan_features(&ctx.gem_settings, None));
    try!(packages::ensure_packages(distro, ctx, &features));

    try!(configure(ctx));

    if pkgs.len() == 0 {
        return Ok(());
    }

    let gem_exe = ctx.gem_settings.gem_exe.clone()
        .unwrap_or(DEFAULT_GEM_EXE.to_owned());

    let mut cmd = try!(command(ctx, &gem_exe));
    cmd.arg("install");
    cmd.args(&["--bindir", BIN_DIR]);

    let no_doc = try!(no_doc_args(ctx));
    cmd.args(&no_doc);

    cmd.args(pkgs);
    try!(run(cmd));
    Ok(())
}

pub fn bundle(distro: &mut Box<Distribution>,
    ctx: &mut Context, info: &GemBundleInfo)
    -> Result<(), StepError>
{
    let features = try!(scan_features(&ctx.gem_settings, Some(info)));
    try!(packages::ensure_packages(distro, ctx, &features));

    try!(configure(ctx));

    let mut cmd = try!(command(ctx, "bundle"));
    cmd.args(&["install", "--system", "--binstubs", BIN_DIR]);

    cmd.arg("--gemfile");
    cmd.arg(&info.gemfile);

    if !info.without.is_empty() {
        cmd.arg("--without");
        cmd.args(&info.without);
    }

    if let Some(ref trust_policy) = info.trust_policy {
        if !VALID_TRUST_POLICIES.contains(&trust_policy.as_ref()) {
            return return Err(From::from(format!(
                "Value of 'GemBundle.trust_policy' must be \
                    '{}', '{}' or '{}', '{}' given",
                VALID_TRUST_POLICIES[0],
                VALID_TRUST_POLICIES[1],
                VALID_TRUST_POLICIES[2],
                trust_policy
            )))
        }
        cmd.arg("--trust-policy");
        cmd.arg(trust_policy);
    }

    run(cmd)
}

pub fn configure(ctx: &mut Context) -> Result<(), String> {
    if ctx.gem_settings.gem_exe.is_none() &&
        ctx.gem_settings.update_gem
    {
        let mut args = vec!(
            DEFAULT_GEM_EXE.to_owned(),
            "update".to_owned(),
            "--system".to_owned(),
        );

        let version = try!(gem_version(ctx));
        if version < GEM_VERSION_WITH_NO_DOCUMENT_OPT {
            args.extend(vec!("--no-rdoc".to_owned(), "--no-ri".to_owned()));
        } else {
            args.push("--no-document".to_owned());
        }

        // Debian based distros doesn't allow updating gem unless this flag is set
        let env = [("REALLY_GEM_UPDATE_SYSTEM", "1")];
        try!(run_command_at_env(ctx, &args, Path::new("/work"), &env));
    }

    let gem_cache = try!(gem_cache_dir(ctx));
    try!(ctx.add_cache_dir(&gem_cache,
                           "gems-cache".to_string()));

    Ok(())
}

pub fn setup_bundler(ctx: &mut Context) -> Result<(), String> {
    try!(configure(ctx));

    let gem_exe = ctx.gem_settings.gem_exe.clone()
        .unwrap_or(DEFAULT_GEM_EXE.to_owned());

    let mut cmd = try!(command(ctx, gem_exe));
    cmd.args(&["install", "bundler"]);

    let no_doc = try!(no_doc_args(ctx));
    cmd.args(&no_doc);

    try!(run(cmd));

    Ok(())
}

pub fn list(ctx: &mut Context) -> Result<(), StepError> {
    let gem_exe = ctx.gem_settings.gem_exe.clone()
        .unwrap_or(DEFAULT_GEM_EXE.to_owned());

    let mut cmd = try!(command(ctx, gem_exe));
    cmd.arg("list");
    cmd.arg("--local");

    try!(capture_stdout(cmd)
        .and_then(|out| {
            File::create("/vagga/container/gems-list.txt")
            .and_then(|mut f| f.write_all(&out))
            .map_err(|e| format!("Error dumping gems package list: {}", e))
        }));
    Ok(())
}
