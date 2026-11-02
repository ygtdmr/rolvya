-- Bounded project-scoped operational reads; existing journals remain authority.
CREATE INDEX panel_worker_revision ON worker_events(project_id,sequence);
CREATE INDEX panel_provider_revision ON provider_security_events(project_id,sequence);
CREATE INDEX panel_secret_revision ON secret_events(project_id,sequence);
CREATE INDEX panel_git_revision ON git_publication_events(project_id);
CREATE INDEX panel_verification_revision ON coding_verifications(project_id);
CREATE INDEX panel_rejection_revision ON coding_rejections(project_id);
CREATE INDEX panel_aws_revision ON aws_execution_events(project_id);
CREATE INDEX panel_attempt_page ON task_attempts(project_id,plan_id,version,admission_event);
CREATE INDEX panel_model_page ON provider_project_grants(project_id);
CREATE INDEX panel_infrastructure_page ON aws_infrastructure_accounts(project_id);
