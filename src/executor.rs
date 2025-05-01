use std::{env, path::PathBuf, process::Command};

pub struct Executor {
    args: Vec<String>
}

impl Executor {
    pub fn new(args: Vec<String>) -> Self {
        Self {
            args
        }
    }

    pub fn exec(&self) -> anyhow::Result<()> {
        let ruby_ver = "3.3.0";
        let vendor_root = PathBuf::from("vendor").join("bundle").join("ruby").join(&ruby_ver);
        let bin_path = vendor_root.join("bin");
    
        // Environment --------------------------------------------------------------
        let rubyopt = {
            let cur = env::var("RUBYOPT").unwrap_or_default();
            if cur.contains("-rbundler/setup") { cur } else { format!("-rbundler/setup {}", cur) }
        };
        let path_val = {
            let orig = env::var("PATH").unwrap_or_default();
            if bin_path.exists() { format!("{}:{}", bin_path.display(), orig) } else { orig }
        };
    
        let status = Command::new(&self.args[0])
            .args(&self.args[1..])
            .env("BUNDLE_GEMFILE", "Gemfile")
            .env("GEM_HOME", &vendor_root)
            .env("GEM_PATH", &vendor_root)
            .env("RUBYOPT", rubyopt)
            .env("PATH", path_val)
            .status()?;
    
        std::process::exit(status.code().unwrap_or(1));
    }
}
