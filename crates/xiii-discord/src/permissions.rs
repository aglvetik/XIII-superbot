pub fn has_any_role(member_role_ids: &[u64], allowed_role_ids: &[u64]) -> bool {
    member_role_ids
        .iter()
        .any(|role_id| allowed_role_ids.contains(role_id))
}
