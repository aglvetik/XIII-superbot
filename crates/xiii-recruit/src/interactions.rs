pub fn accept_button_id(recruit_id: i64) -> String {
    format!("xiii_recruit_accept:{recruit_id}")
}

pub fn reject_button_id(recruit_id: i64) -> String {
    format!("xiii_recruit_reject:{recruit_id}")
}

pub fn extend_button_id(recruit_id: i64) -> String {
    format!("xiii_recruit_extend:{recruit_id}")
}
