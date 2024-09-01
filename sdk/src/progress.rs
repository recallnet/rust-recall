// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::fmt::Write;
use std::sync::Arc;
use std::time::Duration;

use console::Emoji;
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressState, ProgressStyle};
use lazy_static::lazy_static;

pub(crate) static SPARKLE: Emoji<'_, '_> = Emoji("✨ ", ":-)");

lazy_static! {
    static ref SPINNER_STYLE: ProgressStyle =
        ProgressStyle::with_template("{prefix:.bold.dim} {spinner:.green} {wide_msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]);
    static ref PROGRESS_STYLE: ProgressStyle = ProgressStyle::with_template(
        "[{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})"
    )
    .unwrap()
    .with_key("eta", |state: &ProgressState, w: &mut dyn Write| write!(
        w,
        "{:.1}s",
        state.eta().as_secs_f64()
    )
    .unwrap())
    .progress_chars("#>-");
}

/// Create a new progress bar. Use `hide` to hide all child bars.
pub(crate) fn new_multi_bar(hide: bool) -> Arc<MultiProgress> {
    if hide {
        Arc::new(MultiProgress::with_draw_target(ProgressDrawTarget::hidden()))
    } else {
        Arc::new(MultiProgress::new())
    }
}

/// Create a new progress bar.
pub(crate) fn new_progress_bar(size: u64) -> ProgressBar {
    let pb = ProgressBar::new(size);
    pb.set_style(PROGRESS_STYLE.clone());
    pb
}

/// Create a new message bar.
pub(crate) fn new_message_bar() -> ProgressBar {
    let pb = ProgressBar::new(0);
    pb.set_style(SPINNER_STYLE.clone());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}
