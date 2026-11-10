//! Fragmentos SQL compartidos por las operaciones del repositorio.

pub(super) const PLAN_SELECT: &str = r#"
SELECT p.id, p.product_id, pr.key AS product_key, pr.name AS product_name,
       p.key, p.name, p.description, p.status, p.is_trial,
       p.created_at, p.updated_at,
       (SELECT count(*) FROM subscription s WHERE s.plan_id = p.id) AS subscription_count,
       COALESCE((SELECT jsonb_object_agg(f.key, f.value) FROM plan_feature f WHERE f.plan_id = p.id), '{}'::jsonb) AS features,
       COALESCE((SELECT jsonb_object_agg(l.key, l.value) FROM plan_limit l WHERE l.plan_id = p.id), '{}'::jsonb) AS limits
FROM plan p
JOIN product pr ON pr.id = p.product_id
"#;

pub(super) const SUBSCRIPTION_SELECT: &str = r#"
SELECT s.id, s.organization_id, s.product_id, s.plan_id, s.status, s.is_trial,
       s.starts_at, s.ends_at, s.created_at, s.updated_at, s.suspended_at, s.cancelled_at,
       s.renewed_from_subscription_id,
       p.key AS plan_key, p.name AS plan_name, p.is_trial AS plan_is_trial
FROM subscription s
JOIN plan p ON p.id = s.plan_id
"#;
