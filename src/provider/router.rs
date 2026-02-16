// QuectoClaw — Multi-model routing
//
// Routes messages to different models based on configurable keyword patterns.
// Falls back to the default model when no route matches.

use crate::config::ModelRoute;

/// Routes messages to the appropriate model based on keyword patterns.
#[derive(Debug, Clone)]
pub struct ModelRouter {
    routes: Vec<ModelRoute>,
    default_model: String,
}

impl ModelRouter {
    pub fn new(routes: Vec<ModelRoute>, default_model: String) -> Self {
        Self {
            routes,
            default_model,
        }
    }

    /// Resolve which model to use based on the message content.
    /// Checks each route's keywords against the message (case-insensitive).
    /// Returns the first matching route's model, or the default model.
    pub fn resolve_model(&self, message: &str) -> &str {
        let lower = message.to_lowercase();
        for route in &self.routes {
            for keyword in &route.keywords {
                if lower.contains(&keyword.to_lowercase()) {
                    tracing::debug!(
                        keyword = %keyword,
                        model = %route.model,
                        "Route matched"
                    );
                    return &route.model;
                }
            }
        }
        &self.default_model
    }

    /// Check if routing is enabled (has any routes configured).
    pub fn has_routes(&self) -> bool {
        !self.routes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_route(keywords: Vec<&str>, model: &str) -> ModelRoute {
        ModelRoute {
            keywords: keywords.into_iter().map(String::from).collect(),
            model: model.to_string(),
            description: String::new(),
        }
    }

    #[test]
    fn test_resolve_exact_match() {
        let router = ModelRouter::new(
            vec![
                make_route(vec!["code", "programming", "debug"], "gpt-4o"),
                make_route(vec!["translate", "language"], "gpt-4o-mini"),
            ],
            "gpt-4o-mini".to_string(),
        );

        assert_eq!(router.resolve_model("Help me debug this code"), "gpt-4o");
        assert_eq!(
            router.resolve_model("Translate this to French"),
            "gpt-4o-mini"
        );
    }

    #[test]
    fn test_resolve_case_insensitive() {
        let router = ModelRouter::new(
            vec![make_route(vec!["CODE"], "gpt-4o")],
            "default-model".to_string(),
        );
        assert_eq!(router.resolve_model("write some code"), "gpt-4o");
    }

    #[test]
    fn test_resolve_fallback() {
        let router = ModelRouter::new(
            vec![make_route(vec!["code"], "gpt-4o")],
            "gpt-4o-mini".to_string(),
        );
        assert_eq!(router.resolve_model("What is the weather?"), "gpt-4o-mini");
    }

    #[test]
    fn test_no_routes() {
        let router = ModelRouter::new(vec![], "default".to_string());
        assert!(!router.has_routes());
        assert_eq!(router.resolve_model("anything"), "default");
    }

    #[test]
    fn test_first_match_wins() {
        let router = ModelRouter::new(
            vec![
                make_route(vec!["code"], "model-a"),
                make_route(vec!["code"], "model-b"),
            ],
            "default".to_string(),
        );
        assert_eq!(router.resolve_model("write code"), "model-a");
    }
}
