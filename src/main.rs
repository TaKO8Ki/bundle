mod gemfile;

use std::env;
use std::fs;
use std::process;

fn main() {
    println!("Gemfile Parser for Ruby");
    
    let args: Vec<String> = env::args().collect();
    
    let gemfile_path = if args.len() > 1 {
        args[1].clone()
    } else {
        "Gemfile".to_string()
    };
    
    if let Ok(content) = fs::read_to_string(&gemfile_path) {
        println!("Found Gemfile at {}. Processing...", gemfile_path);
        
        match gemfile::parser::parse(&content) {
            Ok(gemfile) => {
                println!("Successfully parsed Gemfile:");
                
                println!("\nSources:");
                for source in &gemfile.sources {
                    if let Some(name) = &source.name {
                        println!("  - {} => {}", name, source.url);
                    } else {
                        println!("  - {}", source.url);
                    }
                }
                
                println!("\nGems:");
                for gem in &gemfile.gems {
                    print!("  - {}", gem.name);
                    if let Some(version) = &gem.version {
                        print!(" ({})", version);
                    }
                    
                    if !gem.options.is_empty() {
                        print!(" with options:");
                        for option in &gem.options {
                            print!(" {}:", option.key);
                            print_option_value(&option.value);
                        }
                    }
                    println!();
                }
                
                println!("\nGroups:");
                for group in &gemfile.groups {
                    println!("  Group: {}", group.names.join(", "));
                    for gem in &group.gems {
                        print!("    - {}", gem.name);
                        if let Some(version) = &gem.version {
                            print!(" ({})", version);
                        }
                        
                        if !gem.options.is_empty() {
                            print!(" with options:");
                            for option in &gem.options {
                                print!(" {}:", option.key);
                                print_option_value(&option.value);
                            }
                        }
                        println!();
                    }
                }
                
                if let Some(ruby_version) = &gemfile.ruby_version {
                    println!("\nRuby Version: {}", ruby_version);
                }
            },
            Err(e) => {
                eprintln!("Error parsing Gemfile: {}", e);
                process::exit(1);
            }
        }
    } else {
        println!("No Gemfile found at: {}", gemfile_path);
    }
}

fn print_option_value(value: &gemfile::ast::OptionValue) {
    use gemfile::ast::OptionValue;
    
    match value {
        OptionValue::String(s) => print!(" \"{}\"", s),
        OptionValue::Boolean(b) => print!(" {}", b),
        OptionValue::Symbol(s) => print!(" :{}", s),
        OptionValue::Array(arr) => {
            print!(" [");
            for (i, v) in arr.iter().enumerate() {
                if i > 0 {
                    print!(", ");
                }
                print_option_value(v);
            }
            print!("]");
        },
        OptionValue::Hash(h) => {
            print!(" {{");
            for (i, (k, v)) in h.iter().enumerate() {
                if i > 0 {
                    print!(", ");
                }
                print!("{}:", k);
                print_option_value(v);
            }
            print!("}}");
        }
    }
}
