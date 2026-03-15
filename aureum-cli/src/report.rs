use aureum::Tree::{self, Leaf, Node};
use aureum::{
    ParsedTomlConfig, ProgramPath, RequirementData, Requirements, TestCaseValidationError, TestId,
    TomlConfigError,
};
use colored::Colorize;
use relative_path::RelativePathBuf;
use std::collections::BTreeMap;

pub fn print_no_config_files() {
    eprintln!(
        "{} No config files found for the given paths",
        "error:".red().bold(),
    );
}

pub fn print_files_found(source_files: &[RelativePathBuf]) {
    let heading = format!("🔍 Found {} config files", source_files.len());
    let tree = Node(
        heading,
        source_files
            .iter()
            .map(|x| str_to_tree(x.as_ref()))
            .collect(),
    );

    print_tree(tree);
}

pub fn print_config_details(
    source_file: RelativePathBuf,
    parsed_toml_configs: &BTreeMap<TestId, ParsedTomlConfig>,
    requirement_data: &RequirementData,
    verbose: bool,
    hide_absolute_paths: bool,
) {
    let mut tests = vec![];

    for (test_id, parsed_toml_config) in parsed_toml_configs {
        let mut categories = vec![];

        if verbose {
            // Program to run
            let program_to_run = match &parsed_toml_config.program_path {
                ProgramPath::NotSpecified => String::from("❌ Not specified"),
                ProgramPath::MissingProgram { requested_path: _ } => String::from("❌ Not found"),
                ProgramPath::ResolvedPath {
                    requested_path: _,
                    resolved_path,
                } => {
                    let path = if hide_absolute_paths {
                        aureum::display_path(resolved_path)
                    } else {
                        resolved_path.display().to_string()
                    };
                    format!("✅ {}", path)
                }
            };

            let nodes = vec![str_to_tree(&program_to_run)];

            let heading = String::from("Program to run");
            categories.push(Node(heading, nodes));

            // Requirements
            let requirements = requirements_map(&parsed_toml_config.requirements, requirement_data);
            if !requirements.is_empty() {
                let heading = String::from("Requirements");
                categories.push(Node(heading, requirements));
            }
        }

        // Validation errors
        let heading = String::from("Validation errors");
        if let Err(validation_errors) = &parsed_toml_config.test_cases {
            let nodes = validation_errors
                .iter()
                .map(|err| str_to_tree(&show_validation_error(err)))
                .collect();

            categories.push(Node(heading, nodes));
        }

        tests.push((test_id, categories))
    }

    let is_root = tests.len() == 1 && tests[0].0.is_root();
    let nodes: Vec<Tree> = if is_root {
        tests.into_iter().next().unwrap().1
    } else {
        tests
            .into_iter()
            .map(|(test_id, children)| Node(format!(":{}", test_id), children))
            .collect()
    };

    let tree = Node(config_heading(source_file), nodes);

    print_tree(tree);
}

pub fn print_toml_config_error(source_file: RelativePathBuf, error: TomlConfigError) {
    let msg = match error {
        TomlConfigError::InvalidTomlSyntax(_) => "Failed to parse config file",
        TomlConfigError::ParseErrors(_) => "Failed to parse config file",
    };
    let tree = Node(config_heading(source_file), vec![str_to_tree(msg)]);

    print_tree(tree);
}

fn print_tree(tree: Tree) {
    let content = aureum::draw_tree(&tree).unwrap_or(String::from("Failed to draw tree\n"));

    eprint!("{}", content); // Already contains newline
    eprintln!()
}

fn config_heading(source_file: RelativePathBuf) -> String {
    format!("📋 {}", source_file)
}

fn requirements_map(requirements: &Requirements, requirement_data: &RequirementData) -> Vec<Tree> {
    let mut categories = vec![];

    if !requirements.files.is_empty() {
        categories.push(Node(
            String::from("Files"),
            requirements
                .files
                .iter()
                .map(|file| {
                    let is_present = requirement_data.get_file(file).is_some();
                    str_to_tree(&format!("{} {}", show_presence(is_present), file))
                })
                .collect(),
        ));
    }

    if !requirements.env_vars.is_empty() {
        categories.push(Node(
            String::from("Environment"),
            requirements
                .env_vars
                .iter()
                .map(|env_var| {
                    let is_present = requirement_data.get_env_var(env_var).is_some();
                    str_to_tree(&format!("{} {}", show_presence(is_present), env_var))
                })
                .collect(),
        ));
    }

    categories
}

fn show_validation_error(validation_error: &TestCaseValidationError) -> String {
    let msg = match validation_error {
        TestCaseValidationError::MissingExternalFile(file_path) => {
            format!("Missing external file '{}'", file_path)
        }
        TestCaseValidationError::MissingEnvVar(var_name) => {
            format!("Missing environment variable '{}'", var_name)
        }
        TestCaseValidationError::FailedToParseString => String::from("Failed to parse string"),
        TestCaseValidationError::ProgramRequired => String::from("The field 'program' is required"),
        TestCaseValidationError::ProgramNotFound(program) => {
            format!("The program '{}' was not found", program)
        }
        TestCaseValidationError::ExpectationRequired => {
            String::from("At least one expectation is required")
        }
    };

    format!("❌ {}", msg)
}

fn show_presence(value: bool) -> String {
    String::from(if value { "✅" } else { "❌" })
}

fn str_to_tree(msg: &str) -> Tree {
    Leaf(vec![msg.to_owned()])
}
