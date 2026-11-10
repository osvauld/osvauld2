"""Measure a live rover preview on this machine; build its release binary through LoadTable first.

No build is invoked here. --loader allows a fetched Linux binary to use this host's ABI-compatible
dynamic loader without modifying the artifact or needing its build machine's Nix store path.
"""
import argparse
import json
import math
import os
import statistics
import subprocess
import tempfile
import time
from pathlib import Path

from osvauld.session import Session, shell_binary

ROOT = Path(__file__).resolve().parent.parent


def nodes(node):
    yield node
    for child in node.get('children', []):
        yield from nodes(child)


def finite(value):
    if value is None:
        return True
    if isinstance(value, float):
        return math.isfinite(value)
    if isinstance(value, dict):
        return all(finite(v) for v in value.values())
    if isinstance(value, list):
        return all(finite(v) for v in value)
    return True


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--seconds', type=float, default=8)
    parser.add_argument('--warmup', type=float, default=3)
    parser.add_argument('--output', type=Path, default=ROOT / 'shots/rover-laptop-performance.json')
    parser.add_argument('--loader', type=Path)
    parser.add_argument('--foreground', action='store_true', help='focus and maximize this preview on Sway')
    parser.add_argument('--mission', action='store_true', help='measure the larger crater mission')
    args = parser.parse_args()
    binary = shell_binary(release=True).resolve()
    os.environ['OSVAULD_PERFORMANCE'] = '1'
    os.environ.pop('OSVAULD_OFFSCREEN', None)
    with tempfile.TemporaryDirectory(prefix='rover-preview-') as directory:
        if args.loader:
            launcher = Path(directory) / 'shell2'
            launcher.write_text('#!/usr/bin/env python3\nimport os,sys\n'
                                f'os.execv({str(args.loader)!r},[{str(args.loader)!r},{str(binary)!r},*sys.argv[1:]])\n')
            launcher.chmod(0o700)
            binary = launcher
        with Session(shell_binary=binary, show_shell_output=True, startup_timeout=30) as session:
            rpc = session.rpc
            rpc.signup('performance preview', 'isolated performance passphrase')
            workspace = rpc.create_workspace('Rover performance')
            item = rpc.create_item(workspace['id'], 'Moon rover', 'app')['id']
            rpc.upload_folder(item, ROOT / 'demo_apps/moon_rover')
            rpc.open_item(item)
            world_id = 'crater-mission' if args.mission else 'lunar-course'
            view_id = 'mission-view' if args.mission else 'course-view'
            drive_id = 'mission-drive' if args.mission else 'rover-drive'
            pause_id = 'mission-pause' if args.mission else 'course-pause'
            # A held native command from the ordinary click handler, not a per-frame Lua driver.
            path = 'mission.lua' if args.mission else 'main.lua'
            old = '\tM.running,M.status = true,"Exploring"\n\tM.send()\n' if args.mission else '\tM.drive()\n'
            source = rpc.read_file_versioned(item, path)
            result = rpc.edit_file(item, path, source['revision'], [{
                'old_text': old,
                'new_text': old+'\trequire("vehicle").apply(M.world,{throttle=0.1})\n',
            }])
            if result['activation'] != 'activated':
                raise RuntimeError(result)
            if args.foreground:
                # Target only the process this session owns, never a guessed screen coordinate.
                criteria = f'[pid={session.process.pid}]'
                sway_env = dict(os.environ)
                if not sway_env.get('SWAYSOCK'):
                    sockets = list(Path(sway_env.get('XDG_RUNTIME_DIR', f'/run/user/{os.getuid()}')).glob(f'sway-ipc.{os.getuid()}.*.sock'))
                    if len(sockets) != 1:
                        raise RuntimeError('Set SWAYSOCK to the compositor socket for this preview')
                    sway_env['SWAYSOCK'] = str(sockets[0])
                subprocess.run(['swaymsg', criteria, 'focus'], env=sway_env, check=True, stdout=subprocess.DEVNULL)
                subprocess.run(['swaymsg', criteria, 'fullscreen', 'enable'], env=sway_env, check=True, stdout=subprocess.DEVNULL)
                time.sleep(0.3)
            if args.mission:
                rpc.click(item, 'mission-enter')
            rpc.click(item, drive_id)
            time.sleep(args.warmup)

            samples = []
            start = time.monotonic()
            print('ROVER_MEASURE_BEGIN', flush=True)
            while time.monotonic() - start < args.seconds:
                stats = rpc.performance()
                tree = rpc.dump_tree(item)
                world = tree['worlds3d'][world_id]
                samples.append({'at': time.monotonic(), 'performance': stats, 'world': world})
                time.sleep(0.5)
            print('ROVER_MEASURE_END', flush=True)
            rpc.click(item, pause_id)
            rpc.save_screenshot(item, args.output.with_suffix('.png'))
            tree = rpc.dump_tree(item)
            objects = next(n['scene3d']['objects'] for n in nodes(tree) if n.get('id') == view_id)
            meshes = {o['mesh_resource']: o['mesh_bytes'] for o in objects}
            first, last = samples[0], samples[-1]
            world = last['world']
            stats = last['performance']
            fps = [s['performance']['fps'] for s in samples if s['performance'].get('fps') is not None]
            duration = last['at'] - first['at']
            hz = (world['tick'] - first['world']['tick']) / duration
            drops = world['dropped_seconds'] - first['world']['dropped_seconds']
            collision_triangles = sum(e['authored']['shape'].get('mesh', {}).get('triangles', 0) for e in world['entities'])
            scene_triangles = sum(o['triangles'] for o in objects)
            wheels = world['wheels']
            setup = [w['authored'] for w in wheels]
            travel_ok = all(
                a['rest']-a['travel']-0.001 <= w['suspension_length'] <= a['rest']+a['travel']+0.001
                for sample in samples for w,a in zip(sample['world']['wheels'],setup)
            )
            p95 = statistics.median(s['performance']['work_ms']['p95'] for s in samples)
            median_fps = statistics.median(fps) if fps else None
            console = rpc.read_console(item)
            summary = {
                'profile': stats['profile'], 'adapter': stats['adapter'], 'viewport': stats['viewport'],
                'scale': stats['scale'], 'sample_seconds': duration, 'samples': len(samples),
                'fps_median': median_fps, 'fps_min': min(fps) if fps else None,
                'work_p95_ms_median': p95, 'physics_hz': hz, 'dropped_seconds': drops,
                'bodies': len(world['entities']), 'wheel_parents': len({w['body'] for w in wheels}), 'wheels': len(wheels),
                'collision_triangles': collision_triangles, 'scene_triangles': scene_triangles,
                'unique_mesh_bytes': sum(meshes.values()), 'scene_objects': len(objects),
                'gpu_ms': None,
                'foreground_requested': args.foreground,
                'world': world_id,
            }
            checks = {
                'release_build': stats['profile'] == 'release',
                'fps_60_within_5_percent': median_fps is not None and median_fps >= 57,
                'work_p95_within_16_67_ms': p95 <= 1000 / 60,
                'physics_120_hz_within_5_percent': abs(hz-120) <= 6,
                'no_catchup_drops': drops <= 1e-6,
                'finite_state': all(finite(s['world']) for s in samples),
                'lunar_gravity': max(abs(a-b) for a,b in zip(world['gravity'],[0,-1.62,0])) < 1e-6,
                'chassis_mass_400_kg': abs(next(e['mass'] for e in world['entities'] if e['authored']['id']=='rover')-400) < 0.01,
                'speed_within_limit': all(
                    math.hypot(e['resolved']['velocity'][0],e['resolved']['velocity'][2]) <= 6.2
                    for s in samples for e in s['world']['entities'] if e['authored']['id']=='rover'),
                'suspension_travel': travel_ok,
                'body_wheel_counts': len(world['entities']) == (37 if args.mission else 4) and len({w['body'] for w in wheels}) == 1 and len(wheels) == 4,
                'collision_triangle_budget': collision_triangles <= 524288,
                'scene_resource_budgets': scene_triangles <= 1048576 and sum(meshes.values()) <= 64*1024*1024 and len(objects) <= 256,
                'clean_console': not console,
            }
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps({'summary': summary, 'checks': checks, 'console': console, 'raw': samples}, indent=2)+'\n')
            print(json.dumps({'summary': summary, 'checks': checks, 'report': str(args.output)}, indent=2), flush=True)
            # Performance shortfalls are report findings, not reasons to discard their artifacts.


if __name__ == '__main__':
    main()
