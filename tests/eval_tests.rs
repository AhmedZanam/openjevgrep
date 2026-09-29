use ojg_core::eval::{calculate_metrics, EvalTask};

#[test]
fn calculates_recall_at_k_and_mrr() {
    let tasks = vec![
        EvalTask {
            query: "auth".to_string(),
            expected_files: vec!["src/auth.rs".to_string()],
        },
        EvalTask {
            query: "cache".to_string(),
            expected_files: vec!["src/cache.rs".to_string()],
        },
    ];
    let rankings = vec![
        vec!["src/auth.rs".to_string(), "src/other.rs".to_string()],
        vec![
            "src/other.rs".to_string(),
            "src/cache.rs".to_string(),
            "src/third.rs".to_string(),
        ],
    ];

    let metrics = calculate_metrics(&tasks, &rankings);
    assert_eq!(metrics.recall_at_1, 0.5);
    assert_eq!(metrics.recall_at_5, 1.0);
    assert_eq!(metrics.recall_at_10, 1.0);
    assert_eq!(metrics.mrr, 0.75);
}
