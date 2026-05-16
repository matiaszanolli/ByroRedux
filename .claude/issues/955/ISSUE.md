# #955: Remove redundant pre-loop cmd_set_depth_bias + cmd_set_depth_compare_op
# draw.rs:1646-1651
# depth_bias: dominated by last_render_layer=None (first batch always fires)
# depth_compare_op: dominated by last_z_function=u8::MAX (first batch always fires)
# Keep depth_test_enable and depth_write_enable (last_z_test=true sentinel)
