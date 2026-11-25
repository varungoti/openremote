const messages = Object.freeze({
  service_not_configured: 'OpenRemote service is not configured in this build.',
  invalid_service_profile: 'The bundled OpenRemote service profile is invalid.',
  connection_failed: 'The desktop connection did not become ready. Check the PC ID, PIN and host activation.',
  operation_failed: 'The desktop operation failed. Check connection status before retrying.',
  operation_cancelled: 'The operation was cancelled and the desktop connection was closed.',
  operation_timed_out: 'The operation timed out and the desktop connection was closed.',
  bridge_closed: 'The agent bridge is closed.',
  bridge_busy: 'Too many desktop operations are queued.',
  not_connected: 'Connect to an activated desktop before using this tool.',
  view_only: 'This desktop session is view-only. Input is unavailable.',
  fresh_frame_required: 'Capture a new screenshot before sending input; each observation is valid once for 15 seconds.',
  invalid_frame: 'The desktop did not return a valid bounded screenshot.',
  invalid_coordinates: 'Coordinates must be inside the captured screenshot.',
  invalid_status: 'The desktop returned an unsupported connection status.',
  invalid_method: 'This desktop operation is not supported.',
  debug_logging_disabled: 'Disable browser debug logging before connecting.'
});

export class BridgeError extends Error {
  constructor(code) {
    super(messages[code] ?? messages.operation_failed);
    this.name = 'BridgeError';
    this.code = Object.hasOwn(messages, code) ? code : 'operation_failed';
  }
}

// Browser/HTTP errors can contain request arguments. Never relay their message,
// stack, URL or cause to MCP output or process diagnostics.
export function publicError(error) {
  const safe = error instanceof BridgeError ? error : new BridgeError('operation_failed');
  return { error: safe.code, message: safe.message };
}
