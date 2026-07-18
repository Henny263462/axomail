/**
 * Default port suggestions for the account-setup dialog.
 *
 * These fill the port field automatically as the user picks a protocol and
 * transport security, and stop applying once the user edits the port by
 * hand (see AccountSetupDialog). They are conveniences only — the backend
 * remains the source of truth and validates the final configuration.
 */
import type { IncomingProtocol, TransportSecurity } from "../../types/mail";

/** Conventional incoming port for a protocol and transport security. */
export function incomingPort(
  protocol: IncomingProtocol,
  security: TransportSecurity,
): number {
  if (protocol === "imap") {
    return security === "tls" ? 993 : 143;
  }
  return security === "tls" ? 995 : 110;
}

/** Conventional outgoing (SMTP submission) port for a transport security. */
export function outgoingPort(security: TransportSecurity): number {
  return security === "tls" ? 465 : 587;
}
