use unshare::Command;

use options::pack::{Options};
use launcher::Context;
use launcher::build::build_container;
use launcher::wrap::Wrapper;
use process_util::convert_status;


pub fn pack_command(context: &Context, args: Vec<String>)
    -> Result<i32, String>
{
    let mut cmdline = args.clone();
    cmdline.insert(0, "vagga _pack_image".to_string());
    let opt = match Options::parse(&cmdline) {
        Ok(x) => x,
        Err(code) => return Ok(code),
    };

    let ver = build_container(context, &opt.name, opt.build_mode)?;

    let mut cmd: Command = Wrapper::new(Some(&ver), &context.settings);
    cmd.map_users_for(
        &context.config.get_container(&opt.name).unwrap(),
        &context.settings)?;
    cmd.gid(0);
    cmd.groups(Vec::new());
    cmd.arg("_pack_image").args(&args);
    cmd.status().map(convert_status)
    .map_err(|e| format!("Error running `vagga_wrapperr _pack_image`: {}", e))
}
