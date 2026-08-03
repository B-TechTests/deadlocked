import Gio from 'gi://Gio';

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const BUS_NAME = 'io.github.avitran0.deadlocked.GamescopeTracker';
const OBJECT_PATH = '/io/github/avitran0/deadlocked/GamescopeTracker';
const INTERFACE_XML = `
<node>
  <interface name="io.github.avitran0.deadlocked.GamescopeTracker">
    <method name="GetGeometry">
      <arg name="pid" type="i" direction="in"/>
      <arg name="found" type="b" direction="out"/>
      <arg name="x" type="i" direction="out"/>
      <arg name="y" type="i" direction="out"/>
      <arg name="width" type="i" direction="out"/>
      <arg name="height" type="i" direction="out"/>
    </method>
  </interface>
</node>`;

class GamescopeTracker {
    constructor() {
        this._windows = new Map();
        this._windowCreatedId = global.display.connect(
            'window-created', (_display, window) => this._track(window));
        for (const window of global.display.list_all_windows())
            this._track(window);

        this._dbus = Gio.DBusExportedObject.wrapJSObject(INTERFACE_XML, this);
        this._dbus.export(Gio.DBus.session, OBJECT_PATH);
        this._nameId = Gio.bus_own_name(
            Gio.BusType.SESSION,
            BUS_NAME,
            Gio.BusNameOwnerFlags.NONE,
            null,
            null,
            null);
    }

    _track(window) {
        const pid = window.get_pid();
        if (pid <= 0 || window.get_wm_class()?.toLowerCase() !== 'gamescope')
            return;

        const record = {window, geometry: null, signals: []};
        const update = () => {
            const {x, y, width, height} = window.get_frame_rect();
            record.geometry = [x, y, width, height];
        };
        record.signals.push(window.connect('position-changed', update));
        record.signals.push(window.connect('size-changed', update));
        record.signals.push(window.connect('unmanaged', () => {
            if (this._windows.get(pid) === record)
                this._windows.delete(pid);
        }));
        update();
        this._windows.set(pid, record);
    }

    GetGeometry(pid) {
        const geometry = this._windows.get(pid)?.geometry;
        return geometry ? [true, ...geometry] : [false, 0, 0, 0, 0];
    }

    destroy() {
        global.display.disconnect(this._windowCreatedId);
        for (const {window, signals} of this._windows.values()) {
            for (const signal of signals)
                window.disconnect(signal);
        }
        this._windows.clear();
        this._dbus.unexport();
        Gio.bus_unown_name(this._nameId);
    }
}

export default class GamescopeTrackerExtension extends Extension {
    enable() {
        this._tracker = new GamescopeTracker();
    }

    disable() {
        this._tracker?.destroy();
        this._tracker = null;
    }
}
