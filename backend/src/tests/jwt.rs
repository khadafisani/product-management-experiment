use super::*;
#[test]
fn rejects_expired_wrong_kind_and_tampered_tokens() {
    let sid = Uuid::new_v4().to_string();
    let (expired, _) = token(1, &sid, "access", now() - 1).unwrap();
    let mut h = HeaderMap::new();
    h.insert(
        header::COOKIE,
        format!("access_token={expired}").parse().unwrap(),
    );
    assert!(read(&h, "access_token", "access").is_err());
    let (refresh, _) = token(1, &sid, "refresh", now() + 900).unwrap();
    h.insert(
        header::COOKIE,
        format!("access_token={refresh}").parse().unwrap(),
    );
    assert!(read(&h, "access_token", "access").is_err());
    let (access, _) = token(1, &sid, "access", now() + 900).unwrap();
    h.insert(
        header::COOKIE,
        format!("access_token={access}").parse().unwrap(),
    );
    assert!(read(&h, "access_token", "access").is_ok());
    let mut bytes = access.into_bytes();
    let last = bytes.len() - 2;
    bytes[last] = if bytes[last] == b'A' { b'B' } else { b'A' };
    h.insert(
        header::COOKIE,
        format!("access_token={}", String::from_utf8(bytes).unwrap())
            .parse()
            .unwrap(),
    );
    assert!(read(&h, "access_token", "access").is_err());
}
