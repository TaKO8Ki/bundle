// use std::{io, path::Path};

// use tokio::{fs::File, io::{AsyncWriteExt, BufWriter}};

// async fn write_lockfile(lock: &Lockfile, path: &Path) -> io::Result<()> {
//     let file = File::create(path).await?;
//     let mut w = BufWriter::new(file);

//     w.write_all(b"GEM").await?;
//     writeln!(w, "  remote: https://rubygems.org")?;
//     writeln!(w, "  specs:")?;
//     for gem in &lock.gems {
//         writeln!(w, "    {} ({})", gem.name, gem.version)?;
//         for dep in &gem.deps {
//             writeln!(w, "      {}", dep)?;
//         }
//     }
//     writeln!(w)?;
//     writeln!(w, "PLATFORMS")?;
//     for p in &lock.platforms {
//         writeln!(w, "  {}", p)?;
//     }
//     writeln!(w)?;
//     writeln!(w, "DEPENDENCIES")?;
//     for dep in &lock.root_deps {
//         writeln!(w, "  {}", dep)?;
//     }
//     writeln!(w)?;
//     writeln!(w, "BUNDLED WITH")?;
//     writeln!(w, "   {}", lock.bundler)?;
//     Ok(())
// }
