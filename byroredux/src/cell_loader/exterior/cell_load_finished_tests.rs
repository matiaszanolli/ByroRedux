/// #6219 / #6102 — every cell-apply job that completes retires the cell's
/// pending `OnReset` (`cell_load_finished`): the tile job and, since #6219,
/// the worldspace persistent CELL job.
#[test]
fn every_exterior_apply_job_retires_its_pending_reset_on_completion() {
    // Test modules are interleaved with production here: strip them all.
    let production = super::production_source();
    for (job, complete) in [
        ("impl PersistentCellApplyJob", "PersistentCellApplyProgress::Complete\n"),
        ("impl ExteriorCellApplyJob", "ExteriorCellApplyProgress::Complete(OneCellLoadInfo"),
    ] {
        let body = production.split_once(job).unwrap_or_else(|| panic!("{job} exists")).1;
        let before_complete = body.split_once(complete).unwrap_or_else(|| panic!("{job} completes")).0;
        let tail = &before_complete[before_complete.len().saturating_sub(1200)..];
        assert!(
            tail.contains("obscript_state::cell_load_finished(world, self.cell_root)"),
            "{job} completes without retiring the cell's pending reset (#6219)"
        );
    }
}
