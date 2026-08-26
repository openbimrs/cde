use openbim_cde_oauth::{
    AccessToken, MemoryTokenStore, StoreError, TokenKey, TokenSet, TokenStore,
};
use std::sync::Arc;

fn token(value: &str) -> TokenSet {
    TokenSet::new(AccessToken::new(value), None, None)
}

#[test]
fn token_keys_reject_empty_control_and_oversized_native_identifiers() {
    for invalid in ["", "   ", "user\nname"] {
        assert_eq!(TokenKey::new(invalid).unwrap_err(), StoreError::InvalidKey);
    }
    assert_eq!(
        TokenKey::new("x".repeat(256)).unwrap_err(),
        StoreError::InvalidKey
    );
}

#[tokio::test]
async fn memory_store_enforces_generation_compare_and_swap() {
    let store = MemoryTokenStore::new();
    let key = TokenKey::new("server/user").unwrap();
    let first = store
        .compare_and_swap(&key, None, token("first"))
        .await
        .unwrap();
    assert_eq!(first.generation(), 1);

    let second = store
        .compare_and_swap(&key, Some(1), token("second"))
        .await
        .unwrap();
    assert_eq!(second.generation(), 2);

    let error = store
        .compare_and_swap(&key, Some(1), token("stale"))
        .await
        .unwrap_err();
    assert_eq!(
        error,
        StoreError::GenerationConflict {
            expected: Some(1),
            actual: Some(2),
        }
    );
    assert_eq!(
        store
            .load(&key)
            .await
            .unwrap()
            .unwrap()
            .token()
            .bearer_token()
            .expose(),
        "second"
    );
}

#[tokio::test]
async fn concurrent_cas_has_exactly_one_winner() {
    let store = Arc::new(MemoryTokenStore::new());
    let key = TokenKey::new("race").unwrap();
    store
        .compare_and_swap(&key, None, token("initial"))
        .await
        .unwrap();

    let mut tasks = Vec::new();
    for value in ["winner-a", "winner-b"] {
        let store = Arc::clone(&store);
        let key = key.clone();
        tasks.push(tokio::spawn(async move {
            store.compare_and_swap(&key, Some(1), token(value)).await
        }));
    }
    let mut successes = 0;
    let mut conflicts = 0;
    for task in tasks {
        match task.await.unwrap() {
            Ok(_) => successes += 1,
            Err(StoreError::GenerationConflict { .. }) => conflicts += 1,
            Err(error) => panic!("unexpected store error: {error}"),
        }
    }
    assert_eq!((successes, conflicts), (1, 1));
    assert_eq!(store.load(&key).await.unwrap().unwrap().generation(), 2);
}
