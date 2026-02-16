use crate::workflow::Workflow;
use std::collections::HashMap;
use std::path::Path;

pub fn parse_workflow(path: &Path, args: &HashMap<String, String>) -> anyhow::Result<Workflow> {
    let content = std::fs::read_to_string(path)?;
    let mut workflow: Workflow = serde_yaml::from_str(&content)?;

    // Variable substitution in steps
    for step in &mut workflow.steps {
        for (key, value) in args {
            let placeholder = format!("{{{{ {} }}}}", key);
            step.prompt = step.prompt.replace(&placeholder, value);
        }
    }

    Ok(workflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_workflow(dir: &Path, filename: &str, content: &str) -> std::path::PathBuf {
        let path = dir.join(filename);
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn test_parse_simple_workflow() {
        let tmp = TempDir::new().unwrap();
        let yaml = r#"
name: test-workflow
description: A simple test
steps:
  - name: step1
    prompt: "Hello world"
  - name: step2
    prompt: "Goodbye"
"#;
        let path = write_workflow(tmp.path(), "test.yaml", yaml);
        let args = HashMap::new();
        let wf = parse_workflow(&path, &args).unwrap();

        assert_eq!(wf.name, "test-workflow");
        assert_eq!(wf.description.as_deref(), Some("A simple test"));
        assert_eq!(wf.steps.len(), 2);
        assert_eq!(wf.steps[0].name, "step1");
        assert_eq!(wf.steps[0].prompt, "Hello world");
        assert_eq!(wf.steps[1].name, "step2");
    }

    #[test]
    fn test_parse_workflow_variable_substitution() {
        let tmp = TempDir::new().unwrap();
        let yaml = r#"
name: deploy
steps:
  - name: build
    prompt: "Build project {{ project }}"
  - name: deploy
    prompt: "Deploy {{ project }} to {{ env }}"
"#;
        let path = write_workflow(tmp.path(), "deploy.yaml", yaml);
        let mut args = HashMap::new();
        args.insert("project".to_string(), "QuectoClaw".to_string());
        args.insert("env".to_string(), "production".to_string());

        let wf = parse_workflow(&path, &args).unwrap();

        assert_eq!(wf.steps[0].prompt, "Build project QuectoClaw");
        assert_eq!(wf.steps[1].prompt, "Deploy QuectoClaw to production");
    }

    #[test]
    fn test_parse_workflow_no_description() {
        let tmp = TempDir::new().unwrap();
        let yaml = r#"
name: minimal
steps:
  - name: only-step
    prompt: "Do something"
"#;
        let path = write_workflow(tmp.path(), "minimal.yaml", yaml);
        let wf = parse_workflow(&path, &HashMap::new()).unwrap();

        assert_eq!(wf.name, "minimal");
        assert!(wf.description.is_none());
        assert_eq!(wf.steps.len(), 1);
    }

    #[test]
    fn test_parse_workflow_with_step_variables() {
        let tmp = TempDir::new().unwrap();
        let yaml = r#"
name: vars
steps:
  - name: step1
    prompt: "Test"
    variables:
      key1: value1
      key2: value2
"#;
        let path = write_workflow(tmp.path(), "vars.yaml", yaml);
        let wf = parse_workflow(&path, &HashMap::new()).unwrap();

        assert_eq!(wf.steps[0].variables.len(), 2);
        assert_eq!(wf.steps[0].variables["key1"], "value1");
    }

    #[test]
    fn test_parse_workflow_invalid_yaml() {
        let tmp = TempDir::new().unwrap();
        let path = write_workflow(tmp.path(), "bad.yaml", "not: [valid: yaml: workflow");
        let result = parse_workflow(&path, &HashMap::new());
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_workflow_missing_file() {
        let path = Path::new("/nonexistent/workflow.yaml");
        let result = parse_workflow(path, &HashMap::new());
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_workflow_unused_args_ignored() {
        let tmp = TempDir::new().unwrap();
        let yaml = r#"
name: simple
steps:
  - name: s1
    prompt: "No placeholders here"
"#;
        let path = write_workflow(tmp.path(), "simple.yaml", yaml);
        let mut args = HashMap::new();
        args.insert("unused".to_string(), "value".to_string());

        let wf = parse_workflow(&path, &args).unwrap();
        assert_eq!(wf.steps[0].prompt, "No placeholders here");
    }
}
