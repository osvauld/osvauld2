# Presenter script

Silent visuals accompany this script. Cue timings are rehearsal defaults, not an audio alignment.

## 1. Two people. Two disconnected copies.

Scene `hook` · suggested visual duration: 26 seconds.

An edit should work immediately, even when the other person is offline.

Imagine Alice and Bob have copies of the same counter. They lose their connection, but each person can still click. Alice clicks once, and Bob clicks once. Both screens now say one. Two clicks happened, so when they reconnect, we want the result to be two. How can the copies agree without losing either person's work?

**Demonstration:** Keep the connection off. Increment Alice, then Bob. Pause on the two totals of one. Let the audience predict the merged result.

**Playback cues:**

- 0.7s  ·  Alice clicks once
- 1.6s  ·  Bob clicks once

**Takeaway:** Local updates can happen independently. Agreement comes later.

## 2. Copying the total loses information.

Scene `naive` · suggested visual duration: 30 seconds.

A scalar says what the total is, but does not say whose contributions it includes.

Suppose we send only the number on the screen. Alice sends one to Bob, who replaces his number with it. Both copies agree on one, but agreement alone is not enough: an increment has disappeared. Adding the incoming total is not a general solution either, because receiving it again would count the same work twice. We need a state representation that remembers enough to merge safely.

**Demonstration:** Press Copy Alice's total. Point out that the displayed totals agree while the click count is two. This scene deliberately demonstrates a failing scalar-copy strategy.

**Playback cues:**

- 1.0s  ·  Copy the scalar; the lost contribution stays lost

**Takeaway:** Convergence alone is insufficient: the representation must preserve the intended updates.

## 3. Keep one contribution per replica.

Scene `components` · suggested visual duration: 32 seconds.

Each replica increments its own slot. The visible counter is the sum of the slots.

Here is a grow-only counter. Instead of storing just one number, each copy keeps a contribution for Alice and a contribution for Bob. Alice can increment her own slot; Bob can increment his. The total shown to the user is the sum. Alice's copy initially knows about Alice's click, and Bob's copy knows about Bob's. Those slots give us the information the plain number was missing.

**Demonstration:** Point to the colored contribution slots. Increment Alice again: her slot changes, while her knowledge of Bob stays at zero.

**Playback cues:**

- 1.0s  ·  Alice's own slot grows; Bob's slot is unchanged

**Takeaway:** Only the owning replica increments a slot. The slots never decrease.

## 4. Merge knowledge, slot by slot.

Scene `merge` · suggested visual duration: 34 seconds.

For each replica's contribution, retain the largest value seen so far.

Now reconnect. We send a snapshot of Alice's state and a snapshot of Bob's. To merge, compare the Alice slots and keep their maximum; do the same for the Bob slots. After both deliveries, both copies know about both clicks. Each total is two. We did not add incoming totals or pick a winning document. We combined the information in corresponding contribution slots.

**Demonstration:** Queue both snapshots. Deliver each one, watching the remote contribution appear. Show max(local slot, incoming slot) and then the sum.

**Playback cues:**

- 0.5s  ·  Queue Alice → Bob
- 1.0s  ·  Queue Bob → Alice
- 1.7s  ·  Bob learns Alice's contribution
- 2.5s  ·  Alice learns Bob's contribution

**Takeaway:** Merge = per-slot maximum. Displayed value = sum of the slots.

## 5. The same message can arrive twice.

Scene `duplicate` · suggested visual duration: 28 seconds.

Learning the same contribution again does not create another increment.

Networks retry messages, so let us deliver the same snapshot again. The result stays two. Taking the maximum of a slot and the same value leaves that slot unchanged. An older snapshot cannot lower it either. This property is called idempotence. The counter tracks each replica's growing contribution, rather than treating every message arrival as a new click.

**Demonstration:** Press Repeat last several times. Point to the increasing delivery count and the unchanged totals.

**Playback cues:**

- 0.8s  ·  Repeat the last delivery
- 1.6s  ·  Repeat it again

**Takeaway:** Repeated delivery changes no contribution. Older state cannot lower a slot.

## 6. Different delivery orders. The same result.

Scene `order` · suggested visual duration: 32 seconds.

Try the oldest or newest snapshot first, including a stale snapshot from Alice.

Here Alice clicked twice and Bob clicked once. There are three snapshots waiting, including an older one from Alice. Deliver them in a different order and the copies still reach three once all relevant state arrives. Per-slot maximum is commutative, associative, and idempotent. Those properties give this state-based counter its safe merge. It only grows: decrements and collaborative text need other CRDT designs.

**Demonstration:** Deliver newest first, then the remaining snapshots. Reset and try oldest first. Both runs finish at three; repeat the stale snapshot and observe no change.

**Playback cues:**

- 0.7s  ·  Deliver the newest snapshot first
- 1.5s  ·  Deliver the stale Alice snapshot
- 2.3s  ·  Deliver Alice's latest snapshot

**Takeaway:** Same relevant state + the same merge rule → the same result. This lesson covers a grow-only counter.
