use std::time::{Duration, Instant};

use glam::Vec2;
use rand::{RngExt as _, rng};

use crate::{
    config::Config,
    cs2::{
        CS2,
        bones::Bones,
        entity::{player::Player, weapon_class::WeaponClass},
    },
    math::angles_to_fov,
    os::mouse::Mouse,
};

#[derive(Default)]
pub struct Triggerbot {
    shot_start: Option<Instant>,
    shot_end: Option<Instant>,
    auto_trigger_ready: Option<Instant>,
    pub active: bool,
}

impl CS2 {
    pub fn triggerbot(&mut self, config: &Config, aimbot_active: bool) {
        let aimbot_config = self.aimbot_config(config);
        let auto_trigger = aimbot_active && aimbot_config.auto_trigger;
        let auto_trigger_delay = aimbot_config.auto_trigger_delay.clone();
        let hotkey = config.aim.triggerbot_hotkey;
        let config = self.triggerbot_config(config);

        if !auto_trigger {
            self.trigger.auto_trigger_ready = None;
        }

        if !config.enabled && !auto_trigger {
            return;
        }

        if !auto_trigger
            && !Self::check_hotkey(&self.input, config.mode, hotkey, &mut self.trigger.active)
        {
            return;
        }

        if self.trigger.shot_start.is_some() || self.trigger.shot_end.is_some() {
            return;
        }

        if auto_trigger && self.trigger.auto_trigger_ready.is_none() {
            self.trigger.auto_trigger_ready =
                Some(Instant::now() + random_delay(&auto_trigger_delay));
        }

        let Some(local_player) = Player::local_player(self) else {
            return;
        };

        if config.flash_check && local_player.is_flashed(self) {
            return;
        }

        if config.scope_check
            && local_player.weapon_class(self) == WeaponClass::Sniper
            && !local_player.is_scoped(self)
        {
            return;
        }

        if config.velocity_check && local_player.velocity(self).length() > config.velocity_threshold
        {
            return;
        }

        let Some(player) = local_player.crosshair_entity(self) else {
            return;
        };

        if !self.is_ffa() && player.team(self) == local_player.team(self) {
            return;
        }

        if config.head_only {
            let head = player.bone_position(self, Bones::Head.u64());

            let target_angle = self.angle_to_target(&local_player, &head, &Vec2::ZERO);
            let view_angles = local_player.view_angles(self);
            let fov = angles_to_fov(&view_angles, &target_angle);

            let head_radius_fov =
                3.5 / (local_player.position(self) - player.position(self)).length() * 100.0;

            if fov > head_radius_fov {
                return;
            }
        }

        let now = Instant::now();
        let delay = shot_delay(&config.delay, self.trigger.auto_trigger_ready, now);
        self.trigger.shot_start = Some(now + delay);
        self.trigger.shot_end = Some(now + delay + Duration::from_millis(config.shot_duration));
        self.trigger.auto_trigger_ready = None;
    }

    pub fn triggerbot_shoot(&mut self, mouse: &mut Mouse) {
        let now = Instant::now();

        if let Some(shot_time) = self.trigger.shot_start
            && now >= shot_time
        {
            mouse.left_press(self.gamescope_display.as_deref());
            self.trigger.shot_start = None;
        }

        if let Some(shot_end) = self.trigger.shot_end
            && now >= shot_end
        {
            mouse.left_release(self.gamescope_display.as_deref());
            self.trigger.shot_end = None;
        }
    }
}

fn random_delay(delay: &std::ops::RangeInclusive<u64>) -> Duration {
    let (start, end) = if delay.start() <= delay.end() {
        (*delay.start(), *delay.end())
    } else {
        (*delay.end(), *delay.start())
    };
    Duration::from_millis(rng().random_range(start..=end))
}

fn shot_delay(
    delay: &std::ops::RangeInclusive<u64>,
    auto_trigger_ready: Option<Instant>,
    now: Instant,
) -> Duration {
    auto_trigger_ready
        .map(|ready| ready.saturating_duration_since(now))
        .unwrap_or_else(|| random_delay(delay))
}
