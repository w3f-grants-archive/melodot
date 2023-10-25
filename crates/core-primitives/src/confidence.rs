// Copyright 2023 ZeroDAO
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

extern crate alloc;
use crate::{String, KZGCommitment};
use alloc::vec::Vec;
use codec::{Decode, Encode};
#[cfg(feature = "std")]
use rand::Rng;
use sp_arithmetic::{traits::Saturating, Permill};

use melo_das_db::traits::DasKv;
use melo_das_primitives::{config::FIELD_ELEMENTS_PER_BLOB, Position, Segment, KZG};

const CHUNK_COUNT: usize = 2 ^ 4;
const SAMPLES_PER_BLOB: usize = FIELD_ELEMENTS_PER_BLOB / CHUNK_COUNT;

#[cfg(feature = "std")]
pub trait ConfidenceSample {
	fn set_sample(&mut self, n: usize);
}

#[derive(Debug, Clone, Default, Decode, Encode)]
pub struct ConfidenceId(Vec<u8>);

impl ConfidenceId {
	pub fn block_confidence(block_hash: Vec<u8>) -> Self {
		Self(block_hash)
	}

	pub fn app_confidence<BlockNum: Decode + Encode + Clone + Sized>(
		block_num: BlockNum,
		app_id: Vec<u8>,
	) -> Self {
		let mut id = app_id.clone();
		id.extend_from_slice(&block_num.encode());
		Self(id)
	}
}

#[derive(Debug, Clone, Default, Decode, Encode)]
pub struct Sample {
	pub position: Position,
	pub is_availability: bool,
}

impl Sample {
	pub fn set_success(&mut self) {
		self.is_availability = true;
	}

	pub fn key<BlockNum: Decode + Encode + Clone + Sized>(
		&self,
		block_num: &BlockNum,
		app_id: u32,
	) -> Vec<u8> {
		let mut key = Vec::new();
		key.extend_from_slice(&block_num.encode());
		key.extend_from_slice(&app_id.to_be_bytes());
		key.extend_from_slice(&self.position.encode());
		key
	}
}

pub const AVAILABILITY_THRESHOLD: f32 = 0.8;

#[derive(Debug, Clone, Decode, Encode, Default)]
pub struct Confidence {
	pub samples: Vec<Sample>,
	pub commitments: Vec<KZGCommitment>,
}

impl Confidence {
	pub fn value(&self, base_factor: Permill) -> Permill {
		let success_count = self.samples.iter().filter(|&sample| sample.is_availability).count();
		calculate_confidence(success_count as u32, base_factor)
	}

	pub fn exceeds_threshold(&self, base_factor: Permill, threshold: Permill) -> bool {
		self.value(base_factor) > threshold
	}

	pub fn save(&self, id: &ConfidenceId, db: &mut impl DasKv) {
		db.set(&id.0, &self.encode());
	}

	pub fn get(id: &ConfidenceId, db: &mut impl DasKv) -> Option<Self>
	where
		Self: Sized,
	{
		db.get(&id.0)
			.and_then(|encoded_data| Decode::decode(&mut &encoded_data[..]).ok())
	}

	pub fn remove(&self, id: &ConfidenceId, db: &mut impl DasKv) {
		db.remove(&id.0);
	}

	pub fn set_sample_success(&mut self, position: Position) {
		if let Some(sample) = self.samples.iter_mut().find(|sample| sample.position == position) {
			sample.set_success();
		}
	}

	pub fn verify_sample(&self, position: Position, segment: &Segment) -> Result<bool, String> {
		let kzg = KZG::default_embedded();
		if position.y >= self.commitments.len() as u32 {
			return Ok(false)
		}
		let commitment = self.commitments[position.y as usize];
		segment.checked()?.verify(&kzg, &commitment, CHUNK_COUNT)
	}
}

#[cfg(feature = "std")]
impl ConfidenceSample for Confidence {
	fn set_sample(&mut self, n: usize) {
		let mut rng = rand::thread_rng();
		let mut positions = Vec::with_capacity(n);

		while positions.len() < n {
			let x = rng.gen_range(0..SAMPLES_PER_BLOB) as u32;
			let y = rng.gen_range(0..self.commitments.len() as u32);

			let pos = Position { x, y };

			if !positions.contains(&pos) {
				positions.push(pos);
			}
		}

		self.samples = positions
			.into_iter()
			.map(|pos| Sample { position: pos, is_availability: false })
			.collect();
	}
}

// fn calculate_confidence(samples: u32, base_factor: f64) -> f64 {
// 	100f64 * (1f64 - base_factor.powi(samples as i32))
// }

fn calculate_confidence(samples: u32, base_factor: Permill) -> Permill {
	let one = Permill::one();
	let base_power_sample = base_factor.saturating_pow(samples as usize);
	one.saturating_sub(base_power_sample)
}
