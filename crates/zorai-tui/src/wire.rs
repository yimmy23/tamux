#![allow(dead_code)]

#[path = "wire_parts/extract_persona_id_to_deserialize_goal_binding.rs"]
mod extract_persona_id_to_deserialize_goal_binding;

#[path = "wire_parts/goal_proof_check_record_to_output_line.rs"]
mod goal_proof_check_record_to_output_line;

pub use extract_persona_id_to_deserialize_goal_binding::*;
pub use goal_proof_check_record_to_output_line::*;
