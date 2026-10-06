//! Open the editor without Bitwig. Close this window and run it again
//! after a rebuild to see layout changes immediately.

use bouncing_ball_plugin::BouncingBall;
use nice_plug::prelude::*;

fn main() {
    let _ = nice_export_standalone::<BouncingBall>();
}
