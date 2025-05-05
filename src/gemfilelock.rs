use std::{io, path::Path};

use tokio::{
    fs::File,
    io::{AsyncWriteExt, BufWriter},
};

use crate::{resolver::Resolver, version::RubyVersion};

pub async fn write_lockfile(
    solutions: Vec<(String, RubyVersion)>,
    resolver: Resolver,
    path: &Path,
) -> io::Result<()> {
    let file = File::create(path).await?;
    let mut w = BufWriter::new(file);

    w.write_all(b"GEM\n").await?;
    w.write_all(b"  remote: https://rubygems.org/\n").await?;
    w.write_all(b"  specs:\n").await?;
    let mut solutions = solutions;
    solutions.sort_by(|a, b| a.0.cmp(&b.0));
    for (pkg, ver) in &solutions {
        if pkg == "root" {
            continue;
        }
        w.write_all(format!("    {} ({})\n", pkg, ver).as_bytes())
            .await?;
        if let Some(deps) = resolver.get_dependencies_str(pkg, ver) {
            let mut deps = deps.clone();
            deps.sort_by(|a, b| a.0.cmp(&b.0));
            for (dg, dr) in deps {
                let mut dr = dr.clone();
                dr.reverse();
                w.write_all(
                    format!(
                        "      {}{}\n",
                        dg,
                        if dr.iter().all(|r| r != ">= 0") {
                            format!(" ({})", dr.join(", "))
                        } else {
                            String::new()
                        }
                    )
                    .as_bytes(),
                )
                .await?;
            }
        }
    }
    w.write_all(b"\n").await?;
    w.write_all(b"PLATFORMS\n").await?;
    w.write_all(b"  ruby\n").await?;
    w.write_all(b"\n").await?;
    w.write_all(b"DEPENDENCIES\n").await?;
    if let Some(deps) =
        resolver.get_dependencies_str(&"root".to_string(), &RubyVersion::new(0, 0, 0))
    {
        let mut deps = deps.clone();
        deps.sort_by(|a, b| a.0.cmp(&b.0));
        for (dg, dr) in deps {
            let mut dr = dr.clone();
            // dr.reverse();
            w.write_all(
                format!(
                    "  {}{}\n",
                    dg,
                    if dr.iter().all(|r| r != ">= 0") {
                        format!(" ({})", dr.join(", "))
                    } else {
                        String::new()
                    }
                )
                .as_bytes(),
            )
            .await?;
        }
    }
    w.write_all(b"\n").await?;
    w.write_all(b"BUNDLED WITH\n").await?;
    w.write_all(b"   2.5.22\n").await?;

    w.flush().await?;
    Ok(())
}
