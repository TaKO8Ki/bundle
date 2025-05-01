#!/usr/bin/env ruby
require "bundler"
require "json"

dsl = Bundler::Dsl.new
dsl.eval_gemfile("Gemfile")

dependencies = dsl.dependencies.map do |dep|
  {
    name: dep.name,
    requirement: dep.requirement.to_s,
    groups: dep.groups,
    source: dep.source ? { type: dep.source.class.name, details: dep.source.to_s } : nil,
    git: dep.git,
    platforms: dep.platforms,
    branch: dep.branch,
  }
end

puts JSON.pretty_generate({ dependencies: dependencies })
