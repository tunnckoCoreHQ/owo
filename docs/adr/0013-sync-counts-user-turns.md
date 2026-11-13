# Sync counts user turns

Message indexes count only user and assistant text, and the bookmark stores the index the last successful ingest reached, so the next `sync start` retries a failed run. While a run is pending, new user turns count from its end, so the turns that follow do not start it again.
