import { test } from "node:test";
import assert from "node:assert/strict";
import { allowedSizes } from "./party.ts";

test("queue policy enforces total group size including the leader", () => {
  const cases = [
    [[420], [1, 2]],
    [[440], [1, 2, 3, 5]],
    [[400, 430, 480, 490, 450, 2400], [1, 2, 3, 4, 5]],
    [[1700, 1710, 1750], [1, 2, 3]],
  ];
  for (const [queues, expected] of cases) for (const queue of queues) {
    const sizes = allowedSizes(queue);
    assert.deepEqual(sizes, expected, `queue ${queue}`);
    assert.equal(sizes.includes(0), false);
    assert.equal(sizes.includes(expected.at(-1) + 1), false);
    const maxFriends = sizes.at(-1) - 1;
    assert.equal(sizes.includes(maxFriends + 1), true);
    assert.equal(sizes.includes(maxFriends + 2), false);
  }
  assert.equal(allowedSizes(440).includes(4), false);
});

test("unsupported queues and the superseded Mayhem ID fail closed", () => {
  for (const id of [0, -1, 2300, 9999, NaN, 420.5]) assert.equal(allowedSizes(id), null);
});


test("current Arena format can reduce the party limit and Flex never admits four", () => {
 assert.deepEqual(allowedSizes(1700,2),[1,2]);
 assert.deepEqual(allowedSizes(1700,3),[1,2,3]);
 assert.deepEqual(allowedSizes(440,4),[1,2,3]);
 assert.deepEqual(allowedSizes(420,16),[1,2]);
});
