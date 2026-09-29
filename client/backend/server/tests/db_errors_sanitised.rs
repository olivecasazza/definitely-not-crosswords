use crossword_server::ctx::sanitised_db_error;

#[test]
fn sqlx_error_text_is_not_serialised_to_callers() {
    let raw = r#"error returned from database: invalid input value for enum \"GameActionTypeEnum\": \"forged\""#;
    let err = sqlx::Error::Protocol(raw.to_string());

    let out = sanitised_db_error("save the submitted letters", &err);

    assert_eq!(out, "save the submitted letters failed");
    assert!(!out.contains("invalid input value"));
    assert!(!out.contains("GameActionTypeEnum"));
    assert!(!out.contains("forged"));
}
