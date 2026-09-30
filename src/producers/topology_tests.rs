use super::leverage_record;
use crate::facts::{FileFacts, module_graph};

#[test]
fn isolated_modules_and_unused_public_surface_do_not_claim_reuse() -> anyhow::Result<()> {
    let mut fact = FileFacts::test_fact("src/lib.rs", "lib");
    for surface in [0, 1, 20] {
        fact.graph.public_api_count = surface;
        let graph = module_graph(std::slice::from_ref(&fact));
        let module = graph
            .modules
            .get(&fact.module_id)
            .ok_or_else(|| anyhow::anyhow!("missing module"))?;
        let row = leverage_record(&graph, module);
        assert_eq!(row["leverage_score"], 0.0);
        assert_eq!(row["public_surface"], surface);
        assert_eq!(row["signals"][0], "no observed consumers");
    }
    Ok(())
}
