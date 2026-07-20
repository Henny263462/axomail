/**
 * Typed wrapper over the iMIP invitation RSVP command.
 *
 * `respondInvitation` records the user's response and sends an iMIP REPLY email
 * to the organizer. It resolves to the outcome DTO in the desktop shell, or to
 * `null` in the browser preview where the IPC bridge is absent (ADR-0005).
 */
import { invokeOrThrow } from "../lib/tauri";
import type { InvitationResponseDto, RsvpChoice } from "../types/mail";

/**
 * Responds to the invitation carried by a message: stores the RSVP and sends
 * the iMIP reply to the organizer.
 *
 * Uses {@link invokeOrThrow} so a backend rejection (e.g. offline, or the
 * message is not a live request) rejects and the reading pane can explain it; a
 * send that was attempted but failed still resolves a DTO with `sent: false`.
 * Resolves `null` in the browser preview.
 */
export function respondInvitation(
  messageId: number,
  choice: RsvpChoice,
): Promise<InvitationResponseDto | null> {
  return invokeOrThrow<InvitationResponseDto>("respond_invitation", {
    messageId,
    choice,
  });
}
