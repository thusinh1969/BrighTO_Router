-- Admin có thể xem lại plaintext client key (yêu cầu user — "Key có thể lấy xem lại").
-- Chỉ lưu plaintext cho key tạo SAU migration này. Key tạo trước đó có key_secret = NULL
-- và reveal sẽ trả lỗi rõ ràng thay vì crash.
ALTER TABLE api_keys ADD COLUMN IF NOT EXISTS key_secret TEXT;
