// Copyright 2023 ZeroDAO

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at

//     http://www.apache.org/licenses/LICENSE-2.0

// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use tracing_subscriber::{fmt, EnvFilter};
use tracing_subscriber::util::SubscriberInitExt;

pub fn init_logger() -> Result<(), Box<dyn std::error::Error>> {
    // Set up the filter to ignore `warn` and below.
    let filter = EnvFilter::new("info");

    // Build and initialize the subscriber with the specified filter.
    fmt::Subscriber::builder()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .without_time()
        .with_target(false)
        .finish()
        .init();

    Ok(())
}
