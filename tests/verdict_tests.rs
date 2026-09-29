use std::path::PathBuf;

use ojg_core::backend::verdict::{
    calibrated_probabilities, prompt_for_noul, temperature_for, truncate_tokens, Calibrator,
    LocalVerdictBackend,
};

#[test]
fn builds_the_verdict_noul_prompt_and_label_order() {
    let (prompt, label_count) = prompt_for_noul("Does this answer the question?", "fn main() {}");

    assert_eq!(label_count, 3);
    assert!(prompt.starts_with(
        "<<LABEL>>true: Does this answer the question?<<LABEL>>false: not Does this answer the question?<<LABEL>>insufficient evidence<<SEP>>"
    ));
    assert!(prompt.ends_with(
        "Context:\nfn main() {}\n\nEvaluate proposition: Does this answer the question?"
    ));
}

#[test]
fn calibrates_and_removes_abstention_logit() {
    let calibrator: Calibrator =
        serde_json::from_str(r#"{"temperature":2.8039,"per_k":{"3":1.0}}"#).unwrap();

    let probabilities = calibrated_probabilities(&[2.0, 0.0, -20.0], 3, &calibrator).unwrap();

    assert_eq!(probabilities.len(), 2);
    assert!((probabilities.iter().sum::<f64>() - 1.0).abs() < 1e-12);
    assert!(probabilities[0] > 0.8);
}

#[test]
fn selects_cardinality_temperature_and_falls_back_to_global() {
    let calibrator: Calibrator =
        serde_json::from_str(r#"{"temperature":2.8039,"per_k":{"3":1.0,"5":4.0}}"#).unwrap();

    assert_eq!(temperature_for(3, &calibrator), 1.0);
    assert_eq!(temperature_for(7, &calibrator), 2.8039);
}

#[test]
fn truncates_token_ids_to_the_model_contract_without_panicking() {
    let tokens: Vec<u32> = (0..600).collect();

    let truncated = truncate_tokens(&tokens, 512);

    assert_eq!(truncated.len(), 512);
    assert_eq!(truncated[0], 0);
    assert_eq!(truncated[511], 511);
}

#[test]
#[ignore = "requires the locally installed verdict-1.4 artifacts"]
fn loads_the_local_model_fixture() {
    let directory = PathBuf::from(std::env::var("OJG_VERDICT_MODEL_DIR").unwrap());
    let files = ojg_core::model::ModelFiles {
        model_path: directory.join("model.onnx"),
        tokenizer_path: directory.join("tokenizer.json"),
        calibrator_path: directory.join("calibrator.json"),
        directory,
    };

    LocalVerdictBackend::load(files).unwrap();
}
