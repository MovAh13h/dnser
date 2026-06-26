# dnser-resolver

Forwarding DNS resolver. Given a query, it races the request across every configured upstream over UDP and returns the first valid response, dropping later-arriving duplicates and falling through to the next upstream on timeout or transport error. Allocates DNS message IDs per upstream socket and tracks in-flight queries so concurrent requests don't collide.

Does **not** do full recursive resolution (no root-hint following, no NS chasing, no CNAME unwinding) — callers should point it at recursive upstreams.
</content>
