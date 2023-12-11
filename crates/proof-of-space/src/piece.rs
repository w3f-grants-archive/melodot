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
use crate::{
	BlsScalar, CellMetadata, DasKv, Decode, Encode, FarmerId, KZGProof, Segment, Vec,
	XValueManager, YPos, ZValueManager, FIELD_ELEMENTS_PER_SEGMENT,
};
#[cfg(feature = "std")]
use anyhow::{anyhow, Ok, Result};
use melo_das_primitives::Position;
use scale_info::TypeInfo;

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct Piece<BlockNumber>
where
	BlockNumber: Clone + sp_std::hash::Hash,
{
	pub metadata: PieceMetadata<BlockNumber>,
	pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum PiecePosition {
	Row(u32),
	Column(u32),
}

impl PiecePosition {
	pub fn to_u32(&self) -> u32 {
		match self {
			PiecePosition::Row(row) => *row,
			PiecePosition::Column(column) => *column,
		}
	}

	pub fn from_row(position: &Position) -> Self {
		Self::Row(position.x)
	}

	pub fn from_column(position: &Position) -> Self {
		Self::Column(position.y)
	}
}

impl Default for PiecePosition {
	fn default() -> Self {
		PiecePosition::Row(0)
	}
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct PieceMetadata<BlockNumber>
where
	BlockNumber: Clone + sp_std::hash::Hash,
{
	pub block_num: BlockNumber,
	pub pos: PiecePosition,
}

impl<BlockNumber> PieceMetadata<BlockNumber>
where
	BlockNumber: Clone + sp_std::hash::Hash + Encode,
{
	pub fn new(block_num: BlockNumber, pos: PiecePosition) -> Self {
		Self { block_num, pos }
	}

	pub fn key(&self) -> Vec<u8> {
		Encode::encode(&self)
	}
}

impl<BlockNumber> Piece<BlockNumber>
where
	BlockNumber: Clone + sp_std::hash::Hash + Encode + Decode + PartialEq,
{
	pub fn key(&self) -> Vec<u8> {
		Encode::encode(&self.metadata)
	}

	pub fn new(blcok_num: BlockNumber, pos: PiecePosition, segments: &[Segment]) -> Self {
		let metadata = PieceMetadata { block_num: blcok_num, pos };
		Self { metadata, segments: segments.to_vec() }
	}

	pub fn x_values_iterator<'a>(
		&'a self,
		farmer_id: &'a FarmerId,
	) -> impl Iterator<Item = (u32, &BlsScalar)> + 'a {
		self.segments.iter().flat_map(move |segment| {
			segment.content.data.iter().map(move |bls_scalar| {
				let y = XValueManager::<BlockNumber>::calculate_y(farmer_id, bls_scalar);
				(y, bls_scalar)
			})
		})
	}

	pub fn cell(&self, pos: u32) -> Option<(BlsScalar, KZGProof)> {
		let index = pos / FIELD_ELEMENTS_PER_SEGMENT as u32;
		if index >= self.segments.len() as u32 {
			return None
		}
		let data_index = pos % FIELD_ELEMENTS_PER_SEGMENT as u32;
		let segment = &self.segments[index as usize];
		Some((segment.content.data[data_index as usize], segment.content.proof))
	}

	#[cfg(feature = "std")]
	pub fn get_cell(
		metadata: &CellMetadata<BlockNumber>,
		db: &mut impl DasKv,
	) -> Result<Option<(BlsScalar, KZGProof)>> {
		db.get(&metadata.piece_metadata.key())
			.map(|data| {
				Decode::decode(&mut &data[..])
					.map_err(|e| anyhow!("Failed to decode Piece from database: {}", e))
					.map(|piece: Piece<BlockNumber>| piece.cell(metadata.pos))
			})
			.transpose()
			.map(|opt| opt.flatten())
	}

	#[cfg(feature = "std")]
	pub fn save(&self, db: &mut impl DasKv, farmer_id: &FarmerId) -> Result<()> {
		let metadata_clone = self.metadata.clone();
		db.set(&self.key(), &self.encode());

		self.x_values_iterator(farmer_id).enumerate().try_for_each(
			|(index, (x, bls_scalar_ref))| {
				let cell_metadata =
					CellMetadata { piece_metadata: metadata_clone.clone(), pos: index as u32 };

				let x_value_manager =
					XValueManager::<BlockNumber>::new(&metadata_clone, index as u32, x);

				let x_pos = YPos::from_u32(index as u32);

				x_value_manager.save(db);

				let match_cells = x_value_manager.match_cells(db)?;

				for mc in match_cells {
					if let Some((match_cell_data, _)) = Self::get_cell(&mc, db)? {
						match x_pos {
							YPos::Left(_) => {
								let z_value_manager = ZValueManager::new(
									&cell_metadata,
									&mc,
									bls_scalar_ref,
									&match_cell_data,
								);
								z_value_manager.save(db);
							},
							YPos::Right(_) => {
								let z_value_manager = ZValueManager::new(
									&cell_metadata,
									&mc,
									&match_cell_data,
									bls_scalar_ref,
								);
								z_value_manager.save(db);
							},
						}
					}
				}
				Ok(())
			},
		)?;
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use melo_das_db::mock_db::MockDb;

	#[test]
	fn test_piece_creation_and_key() {
		let block_num = 123;
		let position = PiecePosition::Row(1);
		let segment = Segment::default();
		let piece = Piece::new(block_num, position.clone(), &[segment]);

		assert_eq!(piece.metadata.block_num, block_num);
		assert_eq!(piece.metadata.pos, position);
		assert!(piece.segments.len() == 1);

		let key = piece.key();
		assert!(!key.is_empty());
	}

	#[test]
	fn test_piece_position() {
		let row_pos = PiecePosition::Row(10);
		assert_eq!(row_pos.to_u32(), 10);

		let col_pos = PiecePosition::Column(20);
		assert_eq!(col_pos.to_u32(), 20);

		let position = Position { x: 5, y: 15 };
		let row_position = PiecePosition::from_row(&position);
		assert_eq!(row_position, PiecePosition::Row(5));

		let col_position = PiecePosition::from_column(&position);
		assert_eq!(col_position, PiecePosition::Column(15));
	}

	#[test]
	fn test_piece_save() {
		let mut db = MockDb::new();
		let block_num = 123;
		let position = PiecePosition::Row(1);
		let segment = Segment::default();
		let piece = Piece::new(block_num, position, &[segment]);

		let farmer_id = FarmerId::default();

		assert!(piece.save(&mut db, &farmer_id).is_ok());

		let key = piece.key();
		assert!(db.contains(&key));

		if let Some(encoded_data) = db.get(&key) {
			let decoded_piece = Piece::<u32>::decode(&mut &encoded_data[..]).expect("Decode error");
			assert_eq!(decoded_piece, piece);
		} else {
			panic!("Piece not found in database");
		}
	}
}
