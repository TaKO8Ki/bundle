use pest::Parser;
use pest::iterators::Pair;
use pest_derive::Parser;

use super::ast::*;

#[derive(Parser)]
#[grammar = "gemfile/gemfile.pest"]
pub struct GemfileParser;

pub fn parse(input: &str) -> Result<Gemfile, pest::error::Error<Rule>> {
    let parse_result = GemfileParser::parse(Rule::gemfile, input)?;

    let mut gemfile = Gemfile {
        sources: Vec::new(),
        gems: Vec::new(),
        ruby_version: None,
        groups: Vec::new(),
    };

    for pair in parse_result {
        match pair.as_rule() {
            Rule::gemfile => {
                for statement in pair.into_inner() {
                    match statement.as_rule() {
                        Rule::source_statement => {
                            if let Some(source) = parse_source_statement(statement) {
                                gemfile.sources.push(source);
                            }
                        }
                        Rule::gem_statement => {
                            if let Some(gem) = parse_gem_statement(statement) {
                                gemfile.gems.push(gem);
                            }
                        }
                        Rule::ruby_version => {
                            gemfile.ruby_version = parse_ruby_version(statement);
                        }
                        Rule::group_block => {
                            if let Some(group) = parse_group_block(statement) {
                                gemfile.groups.push(group);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    Ok(gemfile)
}

fn parse_source_statement(pair: Pair<Rule>) -> Option<Source> {
    let mut url = None;

    for inner_pair in pair.into_inner() {
        if inner_pair.as_rule() == Rule::string_literal {
            url = Some(parse_string_literal(inner_pair));
        }
    }

    if let Some(url_str) = url {
        Some(Source {
            name: None,
            url: url_str,
        })
    } else {
        None
    }
}

fn parse_gem_statement(pair: Pair<Rule>) -> Option<GemStatement> {
    let mut name = None;
    let mut version = None;
    let mut options = Vec::new();

    for inner_pair in pair.into_inner() {
        match inner_pair.as_rule() {
            Rule::string_literal => {
                name = Some(parse_string_literal(inner_pair));
            }
            Rule::version_option => {
                for option_pair in inner_pair.into_inner() {
                    if option_pair.as_rule() == Rule::string_literal {
                        version = Some(parse_string_literal(option_pair));
                    }
                }
            }
            Rule::key_value_option => {
                if let Some((key, value)) = parse_key_value_option(inner_pair) {
                    println!("aaaaaaaaaaaaaaaaaaaa");
                    options.push(GemOption { key, value });
                }
            }
            _ => {}
        }
    }

    if let Some(name_str) = name {
        Some(GemStatement {
            name: name_str,
            version,
            options,
        })
    } else {
        None
    }
}

fn parse_key_value_option(pair: Pair<Rule>) -> Option<(String, OptionValue)> {
    let mut key = None;
    let mut value = None;

    for inner_pair in pair.into_inner() {
        match inner_pair.as_rule() {
            Rule::symbol_or_name => {
                if key.is_none() {
                    key = Some(parse_symbol_or_name(inner_pair));
                }
            }
            Rule::option_value => {
                panic!("bbbbbbbbbbbbbbbbbbbbbbbb");
                value = Some(parse_option_value(inner_pair));
            }
            _ => {}
        }
    }

    // panic!("cccccccccccccccccc: {:?}, {:?}", key, value);

    if let (Some(key_str), Some(val)) = (key, value) {
        Some((key_str, val))
    } else {
        None
    }
}

fn parse_option_value(pair: Pair<Rule>) -> OptionValue {
    for inner_pair in pair.into_inner() {
        match inner_pair.as_rule() {
            Rule::string_literal => {
                return OptionValue::String(parse_string_literal(inner_pair));
            }
            Rule::symbol_or_name => {
                return OptionValue::Symbol(parse_symbol_or_name(inner_pair));
            }
            Rule::array_value => {
                return parse_array_value(inner_pair);
            }
            _ => {}
        }
    }

    // Default
    OptionValue::Boolean(false)
}

fn parse_array_value(pair: Pair<Rule>) -> OptionValue {
    let mut values = Vec::new();

    for inner_pair in pair.into_inner() {
        if inner_pair.as_rule() == Rule::symbol_array_items {
            for item_pair in inner_pair.into_inner() {
                if item_pair.as_rule() == Rule::symbol_or_name {
                    values.push(OptionValue::Symbol(parse_symbol_or_name(item_pair)));
                }
            }
        }
    }

    OptionValue::Array(values)
}

fn parse_ruby_version(pair: Pair<Rule>) -> Option<String> {
    for inner_pair in pair.into_inner() {
        if inner_pair.as_rule() == Rule::string_literal {
            return Some(parse_string_literal(inner_pair));
        }
    }

    None
}

fn parse_group_block(pair: Pair<Rule>) -> Option<GroupBlock> {
    let mut names = Vec::new();
    let mut gems = Vec::new();

    for inner_pair in pair.into_inner() {
        match inner_pair.as_rule() {
            Rule::symbol_or_name => {
                names.push(parse_symbol_or_name(inner_pair));
            }
            Rule::block_content => {
                for statement_pair in inner_pair.into_inner() {
                    if statement_pair.as_rule() == Rule::statement {
                        for gem_pair in statement_pair.into_inner() {
                            if gem_pair.as_rule() == Rule::gem_statement {
                                if let Some(gem) = parse_gem_statement(gem_pair) {
                                    gems.push(gem);
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if !names.is_empty() {
        Some(GroupBlock { names, gems })
    } else {
        None
    }
}

fn parse_string_literal(pair: Pair<Rule>) -> String {
    let content = pair.as_str();
    // Strip quotes - handles both single and double quotes
    content[1..content.len() - 1].to_string()
}

fn parse_symbol_or_name(pair: Pair<Rule>) -> String {
    let content = pair.as_str();

    // Strip the colon if it's a symbol
    if content.starts_with(':') {
        content[1..].to_string()
    } else {
        content.to_string()
    }
}
