#[cfg(test)]
mod tests {
    use crate::gemfile::parser;

    #[test]
    fn test_source_statement() {
        let input = "source 'https://rubygems.org'\n";

        let result = parser::parse(input);
        assert!(
            result.is_ok(),
            "Failed to parse Gemfile: {:?}",
            result.err()
        );

        let gemfile = result.unwrap();
        assert_eq!(gemfile.sources.len(), 1);
        assert_eq!(gemfile.sources[0].url, "https://rubygems.org");
    }

    #[test]
    fn test_gem_statement() {
        let input = "gem 'rails', '6.1.3'\n";

        let result = parser::parse(input);
        assert!(
            result.is_ok(),
            "Failed to parse Gemfile: {:?}",
            result.err()
        );

        let gemfile = result.unwrap();
        assert_eq!(gemfile.gems.len(), 1);
        assert_eq!(gemfile.gems[0].name, "rails");
        assert!(gemfile.gems[0].version.is_some());
        assert_eq!(gemfile.gems[0].version.as_ref().unwrap(), "6.1.3");
    }

    #[test]
    fn test_gem_with_options() {
        let input = "gem 'byebug', platforms: :mri\n";

        let result = parser::parse(input);
        assert!(
            result.is_ok(),
            "Failed to parse Gemfile: {:?}",
            result.err()
        );

        let gemfile = result.unwrap();
        assert_eq!(gemfile.gems.len(), 1);
        assert_eq!(gemfile.gems[0].name, "byebug");
        assert_eq!(gemfile.gems[0].options.len(), 1);
        assert_eq!(gemfile.gems[0].options[0].key, "platforms");

        // Check that we have the platforms array
        match &gemfile.gems[0].options[0].value {
            crate::gemfile::ast::OptionValue::Array(platforms) => {
                assert_eq!(platforms.len(), 3);

                match &platforms[0] {
                    crate::gemfile::ast::OptionValue::Symbol(s) => assert_eq!(s, "mri"),
                    _ => panic!("Expected symbol"),
                }

                match &platforms[1] {
                    crate::gemfile::ast::OptionValue::Symbol(s) => assert_eq!(s, "mingw"),
                    _ => panic!("Expected symbol"),
                }

                match &platforms[2] {
                    crate::gemfile::ast::OptionValue::Symbol(s) => assert_eq!(s, "x64_mingw"),
                    _ => panic!("Expected symbol"),
                }
            }
            _ => panic!("Expected array"),
        }
    }

    #[test]
    fn test_multiple_statements() {
        let input = "source 'https://rubygems.org'\ngem 'rails', '6.1.3'\n";

        let result = parser::parse(input);
        assert!(
            result.is_ok(),
            "Failed to parse Gemfile: {:?}",
            result.err()
        );

        let gemfile = result.unwrap();
        assert_eq!(gemfile.sources.len(), 1);
        assert_eq!(gemfile.gems.len(), 1);
    }

    #[test]
    fn test_ruby_version() {
        let input = "ruby '2.7.2'\n";

        let result = parser::parse(input);
        assert!(
            result.is_ok(),
            "Failed to parse Gemfile: {:?}",
            result.err()
        );

        let gemfile = result.unwrap();
        assert!(gemfile.ruby_version.is_some());
        assert_eq!(gemfile.ruby_version.as_ref().unwrap(), "2.7.2");
    }

    #[test]
    fn test_group_block() {
        let input = "group :development do\n  gem 'web-console'\nend\n";

        let result = parser::parse(input);
        assert!(
            result.is_ok(),
            "Failed to parse Gemfile: {:?}",
            result.err()
        );

        let gemfile = result.unwrap();
        assert_eq!(gemfile.groups.len(), 1);
        assert_eq!(gemfile.groups[0].names.len(), 1);
        assert_eq!(gemfile.groups[0].names[0], "development");
        assert_eq!(gemfile.groups[0].gems.len(), 1);
        assert_eq!(gemfile.groups[0].gems[0].name, "web-console");
    }

    #[test]
    fn test_complete_gemfile() {
        let input = "
source 'https://rubygems.org'

# Core gems
gem 'rails', '~> 6.1.3'
gem 'pg', '>= 1.1'
gem 'puma', '~> 5.0'

# Frontend
gem 'sass-rails', '>= 6'
gem 'webpacker', '~> 5.0'

# Authentication
gem 'devise'

group :development, :test do
  gem 'byebug', platforms: [:mri, :mingw, :x64_mingw]
  gem 'rspec-rails'
end

group :development do
  gem 'web-console', '>= 4.1.0'
  gem 'rack-mini-profiler', '~> 2.0'
  gem 'listen', '~> 3.3'
end

group :test do
  gem 'capybara', '>= 3.26'
  gem 'selenium-webdriver'
  gem 'webdrivers'
end

ruby '2.7.2'
";

        let result = parser::parse(input);
        assert!(
            result.is_ok(),
            "Failed to parse Gemfile: {:?}",
            result.err()
        );

        let gemfile = result.unwrap();
        assert_eq!(gemfile.sources.len(), 1);
        assert_eq!(gemfile.sources[0].url, "https://rubygems.org");

        assert_eq!(gemfile.gems.len(), 6); // Core, Frontend, Authentication
        assert_eq!(gemfile.gems[0].name, "rails");

        assert!(gemfile.ruby_version.is_some());
        assert_eq!(gemfile.ruby_version.as_ref().unwrap(), "2.7.2");

        assert_eq!(gemfile.groups.len(), 3); // development+test, development, test

        // Check development & test group
        let dev_test_group = &gemfile.groups[0];
        assert_eq!(dev_test_group.names.len(), 2);
        assert!(dev_test_group.names.contains(&"development".to_string()));
        assert!(dev_test_group.names.contains(&"test".to_string()));
        assert_eq!(dev_test_group.gems.len(), 2);

        // Check the platforms option in the byebug gem
        let byebug_gem = &dev_test_group.gems[0];
        assert_eq!(byebug_gem.name, "byebug");
        assert_eq!(byebug_gem.options.len(), 1);
        assert_eq!(byebug_gem.options[0].key, "platforms");
    }
}
