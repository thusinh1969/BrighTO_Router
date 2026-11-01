-- DeepSeek-cloud compatibility for local vLLM routes (LLM_guide_needed.md).
--
-- A) Expose a cloud-shaped model card on /v1/models and /v1/models/{id}:
--    display_name, owned_by, input_modalities, output_modalities, effort_levels,
--    effort_default, system_prompt_update. context_window / max_output_tokens
--    reuse the existing context_tokens / max_output_tokens columns.
--
-- B/C/D) The request/response rewrite (chat_template_kwargs, reasoning_content,
--    usage cache tokens) runs only for routes flagged deepseek_compat, i.e. routes
--    fronting a local vLLM. Cloud routes are untouched.
--
-- All columns are nullable / defaulted: existing routes keep today's output.

ALTER TABLE model_routes
    ADD COLUMN IF NOT EXISTS display_name TEXT,
    ADD COLUMN IF NOT EXISTS owned_by TEXT,
    ADD COLUMN IF NOT EXISTS input_modalities TEXT,
    ADD COLUMN IF NOT EXISTS output_modalities TEXT,
    ADD COLUMN IF NOT EXISTS effort_levels TEXT,
    ADD COLUMN IF NOT EXISTS effort_default TEXT,
    ADD COLUMN IF NOT EXISTS system_prompt_update TEXT,
    ADD COLUMN IF NOT EXISTS deepseek_compat BOOLEAN NOT NULL DEFAULT FALSE;

COMMENT ON COLUMN model_routes.display_name IS
    'Human display name for the cloud-shaped model card (NULL = public model_name)';
COMMENT ON COLUMN model_routes.owned_by IS
    'owned_by in the cloud-shaped model card (NULL = omitted)';
COMMENT ON COLUMN model_routes.input_modalities IS
    'JSON array of input modalities, e.g. ["text","image"] (NULL = omitted)';
COMMENT ON COLUMN model_routes.output_modalities IS
    'JSON array of output modalities, e.g. ["text"] (NULL = omitted)';
COMMENT ON COLUMN model_routes.effort_levels IS
    'JSON array of supported reasoning effort levels, e.g. ["low","high","max"] (NULL = omitted)';
COMMENT ON COLUMN model_routes.effort_default IS
    'Default reasoning effort level; must be one of effort_levels (NULL = omitted)';
COMMENT ON COLUMN model_routes.system_prompt_update IS
    'api_capabilities.anthropic_messages.system_prompt_update, e.g. "in-history" (NULL = omitted)';
COMMENT ON COLUMN model_routes.deepseek_compat IS
    'Local vLLM route: accept DeepSeek-cloud thinking/effort params and return cloud response fields';

-- Seed the route the owner's DeepSeek harness targets (LLM_guide_needed.md §A/E).
-- COALESCE keeps any value an operator already set.
UPDATE model_routes SET
    display_name          = COALESCE(display_name, 'DeepSeek-V4.1-Flash'),
    owned_by              = COALESCE(owned_by, 'deepseek'),
    context_tokens        = COALESCE(context_tokens, 1048576),
    max_output_tokens     = COALESCE(max_output_tokens, 393216),
    input_modalities      = COALESCE(input_modalities, '["text","image"]'),
    output_modalities     = COALESCE(output_modalities, '["text"]'),
    effort_levels         = COALESCE(effort_levels, '["low","high","max"]'),
    effort_default        = COALESCE(effort_default, 'high'),
    system_prompt_update  = COALESCE(system_prompt_update, 'in-history'),
    deepseek_compat       = TRUE
WHERE model_name = 'deepseek-v41-flash';
