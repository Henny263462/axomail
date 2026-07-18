/**
 * Mutable form state for one server section of the account-setup dialog.
 *
 * A draft mirrors the fields the user edits before submission; the dialog
 * converts drafts into the immutable {@link CreateAccountInput} shape when
 * it calls the backend.
 */
import type { IncomingProtocol, TransportSecurity } from "../../types/mail";

export interface ServerDraft {
  /** Present for the incoming (IMAP/POP3) section; absent for outgoing. */
  protocol?: IncomingProtocol;
  host: string;
  port: number;
  security: TransportSecurity;
  username: string;
  password: string;
}
