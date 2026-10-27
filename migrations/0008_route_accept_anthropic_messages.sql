-- OpenAI-compatible Chat routes can optionally accept Anthropic Messages at
-- /v1/messages via translation. The public model name keeps serving
-- /v1/chat/completions normally; native anthropic_messages routes are unchanged.
ALTER TABLE model_routes
    ADD COLUMN IF NOT EXISTS accept_anthropic_messages BOOLEAN NOT NULL DEFAULT FALSE;

COMMENT ON COLUMN model_routes.accept_anthropic_messages IS
    'OpenAI-chat route also accepts Anthropic Messages at /v1/messages via translation';
