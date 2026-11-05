"""Drive a real crater sample mission, recover a fall, and reopen its sealed completion record."""
import math
import tempfile
from pathlib import Path

from osvauld.scene3d import assert_3d_pixels
from osvauld.session import Session, shell_binary

ROOT = Path(__file__).resolve().parent.parent


def nodes(tree):
    yield tree
    for child in tree.get('children', []):
        yield from nodes(child)


def text(rpc, item, identity):
    return next(n.get('text', '') for n in nodes(rpc.dump_tree(item)) if n.get('id') == identity)


def body(world, identity):
    return next(e for e in world['entities'] if e['authored']['id'] == identity)


def edit(rpc, item, path, old, new):
    source = rpc.read_file_versioned(item, path)
    return rpc.edit_file(item, path, source['revision'], [{'old_text': old, 'new_text': new}])


with tempfile.TemporaryDirectory(prefix='crater-mission-') as directory:
    store = str(Path(directory) / 'store')
    with Session(shell_binary=shell_binary(), data_dir=store, offscreen=(1000, 800)) as session:
        rpc = session.rpc
        rpc.signup('mission', 'isolated mission passphrase')
        workspace = rpc.create_workspace('Lunar mission')
        item = rpc.create_item(workspace['id'], 'Moon rover', 'app')['id']
        rpc.upload_folder(item, ROOT / 'demo_apps/moon_rover')
        rpc.open_item(item)
        rpc.click_at(*rpc.centre_of('mission-enter'))

        def read():
            return rpc.dump_tree(item)['worlds3d']['crater-mission']

        def rover():
            return body(read(), 'rover')['resolved']

        ready = read()
        assert 'vehicles' not in ready and len(ready['wheels']) == 4
        terrain = body(ready, 'lunar-ground')['authored']['shape']['mesh']
        assert terrain['triangles'] == 8192
        scene = next(n['scene3d'] for n in nodes(rpc.dump_tree(item)) if n.get('id') == 'mission-view')
        visual = next(o for o in scene['objects'] if o['id'] == 'lunar-ground')
        assert visual['mesh_resource'] == terrain['resource']
        assert sum(o['triangles'] for o in scene['objects']) < 50000
        for entity in ready['entities']:
            if entity['authored']['id'].startswith('rock-'):
                drawn = next(o for o in scene['objects'] if o['id'] == entity['authored']['id'])
                assert drawn['mesh_resource'] == entity['authored']['shape']['mesh']['resource']
        assert_3d_pixels(rpc, item, 'mission-view', 'rover', logical_width=1000)
        rpc.click(item, 'mission-interact')
        assert text(rpc, item, 'mission-cargo') == 'Cargo: empty'
        rpc.frame(30)
        assert read() == ready, 'Ready mission advanced'
        rpc.click_at(*rpc.centre_of('mission-drive'))
        rpc.frame(180)
        rpc.save_screenshot(item, ROOT / 'shots/mission-lander.png')

        held = set()

        def keys(wanted):
            for key in sorted(held - wanted):
                rpc.keyboard(key, ' ' if key == 'Space' else key, False)
            for key in sorted(wanted - held):
                rpc.keyboard(key, ' ' if key == 'Space' else key, True)
            held.clear()
            held.update(wanted)

        def drive_to(target):
            fast_checked = False
            for _ in range(300):
                pose = rover()
                x, _, z = pose['position']
                dx, dz = target[0]-x, target[2]-z
                distance = math.hypot(dx, dz)
                speed = math.hypot(pose['velocity'][0], pose['velocity'][2])
                if not fast_checked and distance < 3 and speed > 0.65:
                    cargo_before = text(rpc,item,'mission-cargo')
                    rpc.click(item, 'mission-interact')
                    assert text(rpc,item,'mission-status') != 'Mission complete'
                    assert text(rpc,item,'mission-cargo') == cargo_before, 'collected while moving'
                    fast_checked = True
                if distance < 2.5 and speed < 0.25:
                    keys({'Space'})
                    rpc.frame(30)
                    assert fast_checked, 'fast interaction rejection was not exercised'
                    return
                q = pose['rotation']
                forward = [2*(q[0]*q[2]+q[1]*q[3]), 1-2*(q[0]*q[0]+q[1]*q[1])]
                yaw = math.atan2(forward[0], forward[1])
                error = (math.atan2(dx, dz)-yaw+math.pi) % (2*math.pi)-math.pi
                maximum = 2.4 if distance > 8 else 1.3
                wanted = set()
                if distance < 2.5 or speed > maximum+0.25:
                    wanted.add('Space')
                elif speed < maximum-0.15:
                    wanted.add('KeyW')
                if abs(error) > 0.06 and distance >= 2.5:
                    wanted.add('KeyA' if error > 0 else 'KeyD')
                keys(wanted)
                rpc.frame(12)
            raise AssertionError(('did not reach mission target', target, rover()))

        sample = body(read(), 'sample-zone')['authored']['position']
        home = body(read(), 'home-zone')['authored']['position']
        drive_to(sample)
        keys(set())
        rpc.keyboard('KeyE', 'e', True)
        rpc.keyboard('KeyE', 'e', False)
        assert text(rpc, item, 'mission-cargo') == 'Cargo: sample secured'
        rpc.save_screenshot(item, ROOT / 'shots/mission-sample.png')
        drive_to(home)
        keys(set())
        rpc.click_at(*rpc.centre_of('mission-interact'))
        assert text(rpc, item, 'mission-status') == 'Mission complete'
        assert text(rpc, item, 'mission-cargo') == 'Cargo: empty'
        assert rpc.read_data(item)['crater-mission']['stats']['completed_runs'] == 1
        rpc.click(item, 'mission-interact')
        assert rpc.read_data(item)['crater-mission']['stats']['completed_runs'] == 1
        complete = read()
        rpc.frame(60)
        rpc.screenshot(item)
        rpc.screenshot(item, width=1200, height=900, scale=1)
        assert read() == complete, 'completion/captures advanced physics'
        rpc.save_screenshot(item, ROOT / 'shots/mission-complete.png')
        def camera():
            return next(n['scene3d']['camera'] for n in nodes(rpc.dump_tree(item)) if n.get('id') == 'mission-view')
        follow = camera()
        rpc.click(item,'mission-camera')
        overview = camera()
        assert overview != follow and overview['far'] >= 390
        rpc.keyboard('ArrowRight','',True)
        rpc.frame(30)
        rpc.keyboard('ArrowRight','',False)
        assert camera() != overview, 'map keyboard orbit did not move'
        rpc.click(item,'mission-camera')
        assert camera() == follow
        assert read() == complete, 'paused camera navigation advanced physics'

        # Native input command puts the rover inside the fall zone; recovery is the zone callback.
        original = 'button("mission-reset", "Reset run", M.reset),'
        injection = 'button("mission-reset", "Reset run", function() M.reset(); M.cargo=true; M.world:set("rover",{pos={52,-8,0},velocity={0,-1,0}}); M.running=true; M.key({code="KeyW",down=true}) end),'
        result = edit(rpc, item, 'mission_view.lua', original, injection)
        assert result['activation'] == 'activated', result
        rpc.click(item, 'mission-enter') if any(n.get('id') == 'mission-enter' for n in nodes(rpc.dump_tree(item))) else None
        rpc.click(item, 'mission-reset')
        assert text(rpc,item,'mission-cargo') == 'Cargo: sample secured'
        assert all(w['input']['motor'] and w['input']['motor']['max_torque']==42 for w in read()['wheels'])
        rpc.frame(3)
        recovered = read()
        assert 'Recovered' in text(rpc, item, 'mission-status')
        spawn = body(recovered, 'rover')['authored']['position']
        assert max(abs(a-b) for a,b in zip(rover()['position'], spawn)) < 0.01
        assert max(abs(v) for v in rover()['velocity']) < 0.01
        assert all(w['input'] == {'torque':0,'steering':0,'brake':105,'motor':None} for w in recovered['wheels'])
        assert text(rpc, item, 'mission-cargo') == 'Cargo: empty'
        rpc.frame(120)
        assert read() == recovered, 'recovery must park and pause'
        rpc.click(item,'mission-drive')
        rpc.frame(60)
        assert all(w['input']['motor'] is None for w in read()['wheels']), 'recovered held throttle survived resume'
        rpc.click(item,'mission-pause')
        result = edit(rpc, item, 'mission_view.lua', injection, original)
        assert result['activation'] == 'activated', result
        # Both retained worlds are preserved; only the selected viewport may tick.
        rpc.click(item, 'mission-enter') if any(n.get('id') == 'mission-enter' for n in nodes(rpc.dump_tree(item))) else None
        before = read()
        rpc.click(item, 'mission-course')
        rpc.click(item, 'course-release')
        rpc.frame(120)
        assert read() == before
        assert not rpc.read_console(item), rpc.read_console(item)
        before = rpc.dump_tree(item)['worlds3d']
        failed = edit(rpc,item,'vehicle.lua','mass = 400','mass = -1')
        assert failed['persisted'] and 'failed' in failed['activation'], failed
        assert rpc.dump_tree(item)['worlds3d'] == before, 'failed edit changed retained worlds'
        expected_errors = rpc.read_console(item)
        assert len(expected_errors) == 1 and 'invalid 3D body properties' in expected_errors[0], expected_errors
        fixed = edit(rpc,item,'vehicle.lua','mass = -1','mass = 400')
        assert fixed['activation'] == 'activated', fixed
        after = rpc.dump_tree(item)['worlds3d']
        for world_id, world in before.items():
            assert after[world_id]['tick'] == world['tick']
            for entity in world['entities']:
                assert body(after[world_id],entity['authored']['id'])['resolved'] == entity['resolved']
        assert rpc.read_console(item) == expected_errors, 'repair added an unexpected console error'
    with Session(shell_binary=shell_binary(), data_dir=store, offscreen=(1000, 800)) as session:
        rpc = session.rpc
        rpc.unlock('mission', 'isolated mission passphrase')
        rpc.open_item(item)
        rpc.click(item, 'mission-enter')
        assert text(rpc, item, 'mission-cargo') == 'Cargo: empty'
        assert '1' in text(rpc, item, 'mission-history')
        assert rpc.read_data(item)['crater-mission']['stats']['completed_runs'] == 1
        assert not rpc.read_console(item), rpc.read_console(item)
    print('crater mission ok: physical round trip, collection/delivery, recovery, pause, shared geometry and durable history')
