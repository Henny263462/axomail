import { nextTick, ref } from "vue";
import { describe, expect, it } from "vitest";

import { useMessageSelection } from "../src/composables/useMessageSelection";
import { messageRowId, threadRowId } from "../src/types/selection";

/** A fixed flat list of numeric row ids for the gesture tests. */
function list() {
  return ref<number[]>([10, 20, 30, 40, 50]);
}

describe("useMessageSelection gestures", () => {
  it("plain select replaces the selection and sets the anchor", () => {
    const order = list();
    const sel = useMessageSelection(order);

    sel.select(20);
    expect(sel.selectedIds.value).toEqual([20]);
    expect(sel.anchorId.value).toBe(20);
    expect(sel.count.value).toBe(1);
    expect(sel.isEmpty.value).toBe(false);

    // A second plain select does not accumulate.
    sel.select(40);
    expect(sel.selectedIds.value).toEqual([40]);
    expect(sel.anchorId.value).toBe(40);
  });

  it("toggle adds and removes without touching other rows, re-anchoring", () => {
    const order = list();
    const sel = useMessageSelection(order);

    sel.select(20);
    sel.toggle(40);
    expect(sel.selectedIds.value).toEqual([20, 40]);
    expect(sel.anchorId.value).toBe(40);

    sel.toggle(20);
    expect(sel.selectedIds.value).toEqual([40]);
    expect(sel.anchorId.value).toBe(20);
    expect(sel.isSelected(20)).toBe(false);
    expect(sel.isSelected(40)).toBe(true);
  });

  it("range selects the contiguous run from the anchor, both directions", () => {
    const order = list();
    const sel = useMessageSelection(order);

    sel.select(20); // anchor
    sel.select(40, { range: true });
    expect(sel.selectedIds.value).toEqual([20, 30, 40]);
    // The anchor is preserved, so re-extending the other way pivots on 20.
    expect(sel.anchorId.value).toBe(20);

    sel.select(10, { range: true });
    expect(sel.selectedIds.value).toEqual([10, 20]);
    expect(sel.anchorId.value).toBe(20);
  });

  it("range with no anchor degrades to a plain single-select", () => {
    const order = list();
    const sel = useMessageSelection(order);

    sel.select(30, { range: true });
    expect(sel.selectedIds.value).toEqual([30]);
    expect(sel.anchorId.value).toBe(30);
  });

  it("returns selectedIds in list order regardless of gesture order", () => {
    const order = list();
    const sel = useMessageSelection(order);

    sel.select(50);
    sel.toggle(10);
    sel.toggle(30);
    expect(sel.selectedIds.value).toEqual([10, 30, 50]);
  });

  it("clear empties the selection and the anchor", () => {
    const order = list();
    const sel = useMessageSelection(order);
    sel.select(20);
    sel.toggle(40);

    sel.clear();
    expect(sel.selectedIds.value).toEqual([]);
    expect(sel.count.value).toBe(0);
    expect(sel.isEmpty.value).toBe(true);
    expect(sel.anchorId.value).toBeNull();
  });
});

describe("useMessageSelection reconciliation", () => {
  it("drops ids that vanish from the list and forgets a vanished anchor", async () => {
    const order = list();
    const sel = useMessageSelection(order);
    sel.select(20);
    sel.toggle(40);
    expect(sel.anchorId.value).toBe(40);

    // 40 (also the anchor) and 20 leave the list.
    order.value = [10, 30, 50];
    await nextTick();

    expect(sel.selectedIds.value).toEqual([]);
    expect(sel.anchorId.value).toBeNull();
  });

  it("keeps rows that remain, including across a same-list reload", async () => {
    const order = list();
    const sel = useMessageSelection(order);
    sel.toggle(20);
    sel.toggle(40);

    // Reload the same ids as fresh objects/array; selection survives.
    order.value = [10, 20, 30, 40, 50];
    await nextTick();
    expect(sel.selectedIds.value).toEqual([20, 40]);

    // Drop only 20; 40 stays and the anchor (40) is still present.
    order.value = [10, 30, 40, 50];
    await nextTick();
    expect(sel.selectedIds.value).toEqual([40]);
    expect(sel.anchorId.value).toBe(40);
  });
});

describe("useMessageSelection thread-aware identity", () => {
  it("works over tagged string row ids without collisions", () => {
    // Track 3.4 keys a thread row and a message row that share a numeric id
    // with distinct tagged strings; the selection treats them as opaque.
    const order = ref<string[]>([
      threadRowId(1),
      messageRowId(1),
      messageRowId(2),
    ]);
    const sel = useMessageSelection<string>(order);

    sel.select(threadRowId(1));
    sel.select(messageRowId(2), { range: true });
    expect(sel.selectedIds.value).toEqual([
      threadRowId(1),
      messageRowId(1),
      messageRowId(2),
    ]);
    expect(sel.isSelected(messageRowId(1))).toBe(true);
    // The thread row and the message row with the same numeric id never merge.
    expect(threadRowId(1)).not.toBe(messageRowId(1));
  });
});
