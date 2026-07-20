/**
 * iMIP / iCalendar invitation view models (Track 4.4).
 *
 * Split out of `mail.ts` for the size gate and re-exported from `../types/mail`
 * so `../types/mail` stays the single import surface. `InvitationDto` is
 * referenced by `MessageDetailDto` in `mail.ts`.
 */

/** The RSVP choice a user makes on an invitation card. */
export type RsvpChoice = "accept" | "tentative" | "decline";

/**
 * The reading-pane invitation card for a message carrying a text/calendar
 * invitation. Presentation fields are re-derived on every open; `myStatus` and
 * `cancelled` reflect stored RSVP state, and `canRespond` gates the buttons.
 */
export interface InvitationDto {
  messageId: number;
  /** The event's iCalendar UID. */
  uid: string;
  /** The iMIP method slug: "request" | "reply" | "cancel" | … */
  method: string;
  summary: string | null;
  organizerName: string | null;
  organizerEmail: string | null;
  /** Human-readable, timezone-labeled start/end for display. */
  whenDisplay: string | null;
  /** Event start as Unix seconds (best-effort; exact only for UTC). */
  startsAtUnix: number | null;
  endsAtUnix: number | null;
  location: string | null;
  /** The event agenda (DESCRIPTION). */
  description: string | null;
  /** Stored response: "none" | "accepted" | "tentative" | "declined". */
  myStatus: string;
  /** Whether the event has been cancelled. */
  cancelled: boolean;
  /** Whether the card should offer Accept/Tentative/Decline. */
  canRespond: boolean;
}

/** The result of an RSVP: the new stored status and whether the reply sent. */
export interface InvitationResponseDto {
  myStatus: string;
  sent: boolean;
  error: string | null;
}
