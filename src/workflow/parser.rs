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
