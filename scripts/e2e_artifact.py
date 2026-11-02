"""e2e_artifact.py — the product path of docs/design/laptop-host.md (H9), as one story.

    python3 scripts/e2e_artifact.py          # runs the story in a fresh Net
    OSVAULD_KEEP=1 OSVAULD_SHOTS=shots/story python3 scripts/e2e_artifact.py

Alice asks the agent for a chat app. The agent builds it in her space over the bridge — the
same client an agent session drives. Alice shares the space; the link is all Bob gets. Bob
joins, finds the app, and they talk. Bob closes it, comes back later, and the conversation is
still there. Alice's laptop hosted the whole thing; nothing else ran.

Also the demo: every beat screenshots both sides into $OSVAULD_SHOTS as `story-<n>-<peer>.png`.
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.net import Net, build  # noqa: E402
from e2e_chat_sync import CHAT, PUSH, send, sees, texts  # noqa: E402


def beat(n, item, *peers):
    for p in peers:
        p.shot(f"story-{n}", item)


def story(net: Net) -> None:
    alice = net.peer("alice")
    space = alice.rpc.create_workspace("alice & friends")

    # 1. The agent builds the app in Alice's space. Here: an upload of the chat app's source;
    #    an agent session would write the files one by one with the same verbs.
    app = alice.rpc.create_item(space["id"], "chat", "app")
    alice.rpc.upload_folder(app["id"], CHAT)
    alice.rpc.open_item(app["id"])
    i = app["id"]
    send(alice, i, "the agent just built this")
    beat(1, i, alice)

    # 2. Alice shares. The link is short enough to read out loud.
    link = alice.rpc.share_space(space["id"])
    assert link.startswith("osvauld://s/") and len(link) < 120, link

    # 3. Bob joins with nothing but the link and finds the app by name.
    bob = net.peer("bob")
    joined = bob.rpc.join_space(link)
    assert [it["name"] for it in joined["items"]] == ["chat"], joined
    bob.rpc.open_item(i)
    sees(bob, i, "the agent just built this")
    beat(3, i, alice, bob)

    # 4. They talk, live, both ways.
    send(bob, i, "hi alice, it works")
    sees(alice, i, "hi alice, it works")
    send(alice, i, "told you")
    sees(bob, i, "told you")
    beat(4, i, alice, bob)

    # 5. Bob leaves and comes back later; Alice talked meanwhile. It is all there.
    bob.stop()
    send(alice, i, "bob, you still there?")
    send(alice, i, "guess not")
    bob.restart()
    bob.rpc.open_item(i)
    sees(bob, i, "guess not", timeout=PUSH * 4)
    assert texts(bob, i) == texts(alice, i), f"\n{texts(bob, i)}\n{texts(alice, i)}"
    beat(5, i, alice, bob)

    who = alice.rpc.whoami()
    assert space["id"] in who["hosting"], "alice's laptop hosted the whole thing"


def main() -> int:
    build()
    with Net() as net:
        story(net)
    print("ok      story  alice made it, bob joined by link, both talked, bob came back")
    return 0


if __name__ == "__main__":
    sys.exit(main())
