local C = {}

C.bg = "#0e1116"
C.panel = "#161b22"
C.line = "#27313d"
C.text = "#d6deeb"
C.dim = "#8b98ab"
C.accent = "#6aa8ff"

C.limb = "#3b4757"
C.limb_hot = "#6aa8ff"
C.joint = "#93a6bd"
C.joint_hot = "#ffd479"
C.hand = "#4c5d73"
C.hand_hot = "#7fd1a8"
C.anchor = "#1d2632"

C.frame_w = 440
C.frame_h = 320

-- Reach is 112 + 94 + 62 = 268, so the base sits low and left and the rest pose folds the arm:
-- a straight arm at the wave's worst phase still has to land inside the frame, which nothing clips.
C.base_x = 96
C.base_y = 244

C.rest = { shoulder = -55, elbow = 50, wrist = -40 }
C.wave_amp = { shoulder = 14, elbow = 20, wrist = 26 }

C.upper_len = 112
C.fore_len = 94
C.hand_len = 54
C.hand_gap = 8

C.upper_w0 = 15
C.upper_w1 = 11
C.fore_w0 = 11
C.fore_w1 = 8
C.palm_w = 9

C.r_shoulder = 13
C.r_elbow = 10
C.r_wrist = 7

C.nudge = 6
C.wave_speed = 1.0

-- A press this close to a joint's pivot has no lever arm, so its angle is noise: refuse the swing.
C.grab_min = 5

return C
