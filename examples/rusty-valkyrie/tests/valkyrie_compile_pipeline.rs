use rusty_valkyrie::{
    compile_pipeline::{ANALYSIS_STAGE_ORDER, PROCESSING_STAGE_ORDER, StreamHalf, stream_half},
    *,
};
use nyar_types::{CompileStage, pipeline::STAGE_ORDER};

#[test]
fn analysis_then_processing_covers_full_order() {
    let mut joined: Vec<CompileStage> = ANALYSIS_STAGE_ORDER.to_vec();
    joined.extend_from_slice(PROCESSING_STAGE_ORDER);
    assert_eq!(joined.as_slice(), STAGE_ORDER);
}

#[test]
fn stream_half_splits_at_representation_plan() {
    assert_eq!(stream_half(CompileStage::ValidateMir), StreamHalf::Analysis);
    assert_eq!(stream_half(CompileStage::RepresentationPlan), StreamHalf::Processing);
}
