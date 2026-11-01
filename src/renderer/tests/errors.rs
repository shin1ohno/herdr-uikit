use super::*;
use std::error::Error as _;

#[test]
fn frame_failure_retains_the_typed_source_and_counts() {
    let stats = SceneStats {
        paths: 1,
        shadows: 2,
        quads: 3,
        underlines: 4,
        monochrome: 5,
        polychrome: 6,
        surfaces: 7,
    };
    let error = Error::Frame {
        stats,
        source: Box::new(Error::IntermediateTextureMissing),
    };
    assert!(matches!(
        error
            .source()
            .and_then(|source| source.downcast_ref::<Box<Error>>())
            .map(Box::as_ref),
        Some(Error::IntermediateTextureMissing)
    ));
    assert!(error.to_string().contains("1 paths, 2 shadows, 3 quads"));
}

#[test]
fn pipeline_failure_names_the_function_and_keeps_the_compiler_message() {
    let error = Error::PipelineBuild {
        function: "quad_vertex".into(),
        message: "invalid shader ABI".into(),
    };
    assert_eq!(
        error.to_string(),
        "pipeline build failed for quad_vertex: invalid shader ABI"
    );
    assert!(matches!(error, Error::PipelineBuild { function, message }
        if function == "quad_vertex" && message == "invalid shader ABI"));
}
