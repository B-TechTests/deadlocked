use std::time::{Duration, Instant};

use glam::{Vec2, vec2};
use rand::{RngExt as _, rng};

use crate::{
    config::{Config, aim::TransitionRamp},
    cs2::{
        CS2,
        entity::{player::Player, weapon_class::WeaponClass},
    },
    math::{angles_to_fov, vec2_clamp},
    os::mouse::Mouse,
};

#[derive(Default)]
pub struct Aimbot {
    pub active: bool,
    inertia: Vec2,
    remainder: Vec2,
    ramp_target: usize,
    ramp_start: f32,
    curve_offset: Vec2,
    overshoot_offset: Vec2,
    overshoot_direction: Vec2,
    overshooting: bool,
    last_move: Option<Instant>,
}

impl CS2 {
    pub fn aimbot(&mut self, config: &Config, mouse: &mut Mouse) -> bool {
        let hotkey = config.aim.aimbot_hotkey;
        let config = self.aimbot_config(config);

        if !config.enabled {
            return false;
        }

        if !Self::check_hotkey(&self.input, config.mode, hotkey, &mut self.aim.active) {
            return false;
        }

        let Some(target) = &self.target.player else {
            return false;
        };

        if !target.is_valid(self) {
            return false;
        }

        let Some(local_player) = Player::local_player(self) else {
            return false;
        };

        let weapon_class = local_player.weapon_class(self);
        let disallowed_weapons = [
            WeaponClass::Unknown,
            WeaponClass::Knife,
            WeaponClass::Grenade,
        ];
        if disallowed_weapons.contains(&weapon_class) {
            return false;
        }

        if config.flash_check && local_player.is_flashed(self) {
            return false;
        }

        if config.visibility_check && !target.visible(self, &local_player) {
            return false;
        }

        if local_player.shots_fired(self) < config.start_bullet {
            return false;
        }

        let target_angle = {
            let mut smallest_fov = 360.0;
            let mut smallest_angle = glam::Vec2::ZERO;
            for bone in &config.bones {
                let bone_pos = target.bone_position(self, bone.u64());
                let angle =
                    self.angle_to_target(&local_player, &bone_pos, &self.target.previous_aim_punch);
                let fov = angles_to_fov(&local_player.view_angles(self), &angle);
                if fov < smallest_fov {
                    smallest_fov = fov;
                    smallest_angle = angle;
                }
            }

            smallest_angle
        };

        let view_angles = local_player.view_angles(self);
        if angles_to_fov(&view_angles, &target_angle)
            > (config.fov
                * if config.distance_adjusted_fov {
                    self.distance_scale(self.target.distance)
                } else {
                    1.0
                })
        {
            return false;
        }

        let mut aim_angles = view_angles - target_angle;
        if aim_angles.y < -180.0 {
            aim_angles.y += 360.0
        }
        vec2_clamp(&mut aim_angles);

        let now = Instant::now();
        let new_transition = target.pawn != self.aim.ramp_target
            || self
                .aim
                .last_move
                .is_none_or(|last| now.duration_since(last) > Duration::from_millis(100));
        if new_transition {
            self.aim.ramp_target = target.pawn;
            self.aim.inertia = Vec2::ZERO;
            self.aim.remainder = Vec2::ZERO;
            self.aim.curve_offset = Vec2::ZERO;
            self.aim.overshoot_offset = Vec2::ZERO;
            self.aim.overshoot_direction = Vec2::ZERO;
            self.aim.overshooting = false;
        }
        self.aim.last_move = Some(now);

        let sensitivity = self.get_sensitivity() * local_player.fov_multiplier(self);

        let direct_mouse = vec2(
            aim_angles.y / sensitivity * 45.45,
            -aim_angles.x / sensitivity * 45.45,
        ) + self.target.aimpoint_offset;
        if new_transition {
            let curve = config.movement_curve.max(0.0);
            let direction = direct_mouse.normalize_or_zero();
            self.aim.curve_offset =
                vec2(-direction.y, direction.x) * rng().random_range(-curve..=curve);

            let (min, max) = if config.overshoot.start() <= config.overshoot.end() {
                (*config.overshoot.start(), *config.overshoot.end())
            } else {
                (*config.overshoot.end(), *config.overshoot.start())
            };
            self.aim.overshoot_direction = direction;
            self.aim.overshoot_offset = direction * rng().random_range(min.max(0.0)..=max.max(0.0));
            self.aim.overshooting =
                direction != Vec2::ZERO && self.aim.overshoot_offset != Vec2::ZERO;
            self.aim.ramp_start = (direct_mouse + self.aim.overshoot_offset)
                .length()
                .max(f32::EPSILON);
        } else if self.aim.overshooting
            && direct_mouse.dot(self.aim.overshoot_direction) <= -self.aim.overshoot_offset.length()
        {
            self.aim.overshooting = false;
            self.aim.overshoot_offset = Vec2::ZERO;
            self.aim.curve_offset = Vec2::ZERO;
            self.aim.ramp_start = direct_mouse.length().max(f32::EPSILON);
            self.aim.inertia = Vec2::ZERO;
            self.aim.remainder = Vec2::ZERO;
        }

        let movement_target = direct_mouse + self.aim.overshoot_offset;
        let remaining = (movement_target.length() / self.aim.ramp_start).clamp(0.0, 1.0);
        let progress = 1.0 - remaining;
        let curve = self.aim.curve_offset * (std::f32::consts::PI * progress).sin();
        let mouse_angles = (movement_target + curve) / (config.smooth + 1.0).clamp(1.0, 20.0);

        let alpha = 1.0 - config.inertia.clamp(0.0, 1.0) * 0.5;
        self.aim.inertia += (mouse_angles - self.aim.inertia) * alpha;

        let ramp = match config.transition_ramp {
            TransitionRamp::Linear => remaining,
            TransitionRamp::EaseOut => 1.0 - (1.0 - remaining).powi(2),
            TransitionRamp::SmoothStep => remaining * remaining * (3.0 - 2.0 * remaining),
        };
        let ramp = 0.2 + ramp * 0.8;
        let movement = self.aim.inertia * ramp + self.aim.remainder;
        let ready = movement.trunc();
        self.aim.remainder = movement - ready;
        mouse.move_rel(ready);

        self.recoil.previous = local_player.aim_punch(self);

        true
    }
}
