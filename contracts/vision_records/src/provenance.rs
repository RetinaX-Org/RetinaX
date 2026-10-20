//! Record lineage, provenance export and lifecycle state transitions.

use super::*;

#[contractimpl]
#[allow(clippy::too_many_arguments)]
impl VisionRecordsContract {
    pub fn trace_record_ancestors(
        env: Env,
        record_id: u64,
        max_depth: u32,
    ) -> teye_common::lineage::TraversalResult {
        provenance_graph::trace_ancestors(&env, record_id, max_depth)
    }

    pub fn trace_record_descendants(
        env: Env,
        record_id: u64,
        max_depth: u32,
    ) -> teye_common::lineage::TraversalResult {
        provenance_graph::trace_descendants(&env, record_id, max_depth)
    }

    pub fn export_record_dag(env: Env, record_id: u64, max_depth: u32) -> ProvenanceExport {
        provenance_graph::export_dag(&env, record_id, max_depth)
    }

    pub fn verify_record_provenance(
        env: Env,
        record_id: u64,
        max_depth: u32,
    ) -> teye_common::lineage::VerificationResult {
        provenance_graph::verify_provenance(&env, record_id, max_depth)
    }

    pub fn check_lineage_based_access(
        env: Env,
        record_id: u64,
        requester: Address,
        max_depth: u32,
    ) -> LineageAccessResult {
        let (result, _) =
            provenance_graph::check_lineage_access(&env, record_id, &requester, max_depth);
        result
    }

    pub fn collect_lineage_actors(env: Env, record_id: u64, max_depth: u32) -> Vec<Address> {
        provenance_graph::collect_lineage_actors(&env, record_id, max_depth)
    }

    pub fn transition_record_state(
        env: Env,
        caller: Address,
        record_id: u64,
        to_state: LifecycleState,
        retention_until: u64,
        expires_at: u64,
        prerequisites_met: bool,
    ) -> Result<TransitionRecord, ContractError> {
        caller.require_auth();

        let role = match rbac::get_active_assignment(&env, &caller).map(|a| a.role) {
            Some(Role::Admin) => symbol_short!("ADMIN"),
            Some(Role::Ophthalmologist) => symbol_short!("OPHT"),
            Some(Role::Optometrist) => symbol_short!("PROV"),
            _ => symbol_short!("USER"),
        };

        let ctx = TransitionContext {
            actor: caller,
            actor_role: role,
            now: env.ledger().timestamp(),
            retention_until,
            expires_at,
            prerequisites_met,
        };

        state_machine::apply_transition(
            &env,
            0,
            &EntityKind::VisionRecord,
            record_id,
            to_state,
            ctx,
        )
        .map_err(|_| ContractError::InvalidInput)
    }

    pub fn export_state_machine_dot(env: Env, kind: String) -> String {
        if kind == String::from_str(&env, "vision_record") {
            return state_machine::export_dot(&env, &EntityKind::VisionRecord);
        }
        state_machine::export_dot(&env, &EntityKind::Prescription)
    }
}
