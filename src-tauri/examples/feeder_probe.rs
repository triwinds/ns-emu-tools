//! Developer smoke test: cargo run --example feeder_probe -- install|remove <absolute emulator.exe>
//! `install` explicitly enables the current-user Vulkan layer. Close the emulator first.
use ns_emu_tools_lib::{
    models::graphics_components::{GraphicsApi, GraphicsComponentState},
    services::{
        graphics_components::{self, feeder, packages, planning, vulkan},
        installer::InstallReporter,
    },
};
#[tokio::main]
async fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        return Err("usage: feeder_probe install|remove <absolute emulator.exe>".into());
    }
    let exe = std::path::PathBuf::from(&args[2]);
    match args[1].as_str() {
        "install" => {
            let current = graphics_components::detect(exe.clone(), Some(GraphicsApi::Vulkan));
            let created_reshade = current.reshade_state == GraphicsComponentState::NotInstalled;
            if created_reshade {
                let plan = packages::prepare_official(
                    exe.clone(),
                    GraphicsApi::Vulkan,
                    InstallReporter::new(|_| {}),
                )
                .await?;
                if !plan.blockers.is_empty() || plan.requires_external_overwrite_confirmation {
                    return Err(format!("ReShade conflicts: {:?}", plan.blockers));
                }
                planning::install_with_scope(
                    plan.plan_id.ok_or("ReShade plan missing")?,
                    false,
                    true,
                )?;
            }
            let result = async {
                let plan = feeder::prepare(
                    exe.clone(),
                    GraphicsApi::Vulkan,
                    InstallReporter::new(|_| {}),
                )
                .await?;
                println!("{}", serde_json::to_string_pretty(&plan).unwrap());
                if !plan.blockers.is_empty() {
                    return Err(format!("Feeder conflicts: {:?}", plan.blockers));
                }
                feeder::install(plan.plan_id.ok_or("Feeder plan missing")?)
            }
            .await;
            match result {
                Ok(result) => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
                Err(e) => {
                    if created_reshade {
                        if let Err(r) = vulkan::remove_or_repair(exe, false) {
                            return Err(format!("{e}; ReShade rollback: {r}"));
                        }
                    }
                    return Err(e);
                }
            }
        }
        "remove" => println!(
            "{}",
            serde_json::to_string_pretty(&feeder::remove_or_repair(exe, false)?).unwrap()
        ),
        _ => return Err("unknown operation".into()),
    }
    Ok(())
}
