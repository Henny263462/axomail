import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { respondInvitation } from "../src/api/invitations";
import { getMessageDetail } from "../src/api/messages";
import MessageViewPane from "../src/components/message/MessageViewPane.vue";
import type { InvitationDto, MessageDetailDto } from "../src/types/mail";

vi.mock("../src/api/messages", () => ({
  getMessageDetail: vi.fn(),
  setRemoteContentAllowed: vi.fn(),
  setMessageFlagged: vi.fn(),
}));

vi.mock("../src/api/invitations", () => ({
  respondInvitation: vi.fn(),
}));

const getMessageDetailMock = vi.mocked(getMessageDetail);
const respondInvitationMock = vi.mocked(respondInvitation);

const requestInvitation: InvitationDto = {
  messageId: 501,
  uid: "evt-1@example.org",
  method: "request",
  summary: "Design sync",
  organizerName: "Ada Lovelace",
  organizerEmail: "ada@example.org",
  whenDisplay: "20 Jul 2026, 13:00 – 14:00 (Europe/Berlin)",
  startsAtUnix: 1_784_545_200,
  endsAtUnix: 1_784_548_800,
  location: "Room 5",
  description: "Plan the quarter.",
  myStatus: "none",
  cancelled: false,
  canRespond: true,
};

function detailWith(invitation: InvitationDto | null): MessageDetailDto {
  return {
    id: 501,
    subject: "Invitation: Design sync",
    fromName: "Ada Lovelace",
    fromAddress: "ada@example.org",
    to: [{ name: "Klaus", address: "klaus@theferbers.eu" }],
    cc: [],
    dateUnix: 1_752_790_000,
    flagged: false,
    textBody: "You are invited.",
    sanitizedHtml: null,
    remoteBlockedCount: 0,
    remoteAllowed: false,
    scamFlags: [],
    hadActiveContent: false,
    attachments: [],
    messageId: "evt-1@example.org",
    inReplyTo: [],
    references: [],
    invitation,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("invitation card", () => {
  it("renders the invitation with organizer, time, and RSVP buttons", async () => {
    getMessageDetailMock.mockResolvedValue(detailWith(requestInvitation));
    const wrapper = mount(MessageViewPane, { props: { messageId: 501 } });
    await flushPromises();

    const card = wrapper.get('[aria-label="Calendar invitation"]');
    expect(card.text()).toContain("Design sync");
    expect(card.text()).toContain("ada@example.org");
    expect(card.text()).toContain("13:00 – 14:00 (Europe/Berlin)");
    expect(card.text()).toContain("Room 5");
    expect(card.text()).toContain("Plan the quarter.");
    const labels = card.findAll("button.rsvp").map((b) => b.text());
    expect(labels).toEqual(["Accept", "Tentative", "Decline"]);
    // Honest note: no calendar, so conflicts are not checked.
    expect(card.text()).toContain("Conflicts are not checked");
  });

  it("shows no card for a message that is not an invitation", async () => {
    getMessageDetailMock.mockResolvedValue(detailWith(null));
    const wrapper = mount(MessageViewPane, { props: { messageId: 501 } });
    await flushPromises();

    expect(wrapper.find('[aria-label="Calendar invitation"]').exists()).toBe(
      false,
    );
  });

  it("sends the RSVP and reloads so the recorded response shows", async () => {
    getMessageDetailMock.mockResolvedValueOnce(detailWith(requestInvitation));
    respondInvitationMock.mockResolvedValue({
      myStatus: "accepted",
      sent: true,
      error: null,
    });
    // The reload after a successful send reflects the stored response.
    getMessageDetailMock.mockResolvedValueOnce(
      detailWith({ ...requestInvitation, myStatus: "accepted" }),
    );

    const wrapper = mount(MessageViewPane, { props: { messageId: 501 } });
    await flushPromises();

    const accept = wrapper
      .findAll("button.rsvp")
      .find((button) => button.text() === "Accept");
    if (accept === undefined) throw new Error("Accept button missing");
    await accept.trigger("click");
    await flushPromises();

    expect(respondInvitationMock).toHaveBeenCalledWith(501, "accept");
    // Reloaded (initial load + reload = two detail fetches).
    expect(getMessageDetailMock).toHaveBeenCalledTimes(2);
    expect(wrapper.text()).toContain("You accepted this invitation.");
  });

  it("surfaces a failed send on the card without reloading", async () => {
    getMessageDetailMock.mockResolvedValue(detailWith(requestInvitation));
    respondInvitationMock.mockResolvedValue({
      myStatus: "none",
      sent: false,
      error: "The mail server rejected the reply.",
    });

    const wrapper = mount(MessageViewPane, { props: { messageId: 501 } });
    await flushPromises();

    const decline = wrapper
      .findAll("button.rsvp")
      .find((button) => button.text() === "Decline");
    if (decline === undefined) throw new Error("Decline button missing");
    await decline.trigger("click");
    await flushPromises();

    expect(respondInvitationMock).toHaveBeenCalledWith(501, "decline");
    // No reload on a failed send: still the one initial fetch.
    expect(getMessageDetailMock).toHaveBeenCalledTimes(1);
    const alert = wrapper.get('[role="alert"]');
    expect(alert.text()).toContain("The mail server rejected the reply.");
  });

  it("shows a cancelled notice and no RSVP buttons for a cancelled event", async () => {
    getMessageDetailMock.mockResolvedValue(
      detailWith({
        ...requestInvitation,
        method: "cancel",
        cancelled: true,
        canRespond: false,
      }),
    );
    const wrapper = mount(MessageViewPane, { props: { messageId: 501 } });
    await flushPromises();

    const card = wrapper.get('[aria-label="Calendar invitation"]');
    expect(card.text()).toContain("cancelled");
    expect(card.findAll("button.rsvp")).toHaveLength(0);
  });
});
