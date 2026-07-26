const BITS_PER_BYTE = 8, BITS_PER_VARINT_BYTE = 7, U8_BYTES = 1, U16_BYTES = 2, U32_BYTES = 4, U64_BYTES = 8, U128_BYTES = 16

const de_zig_zag_signed = (n) => (n >> 1n) ^ (-(n & 0b1n))
const zig_zag = (n_bytes, n) => (n << 1n) ^ (n >> BigInt(n_bytes * BITS_PER_BYTE - 1))
const varint_max = (n_bytes) => Math.floor((n_bytes * BITS_PER_BYTE + (BITS_PER_BYTE - 1)) / BITS_PER_VARINT_BYTE)
const max_of_last_byte = (n_bytes) => (1 << (n_bytes * BITS_PER_BYTE) % 7) - 1
const to_number_if_safe = (n) => Number.MAX_SAFE_INTEGER < ((n < 0n) ? -n : n) ? n : Number(n)
const varint = (n_bytes, n) => { let value = BigInt(n), out = []; for (let i = 0; i < varint_max(n_bytes); i++) { out.push(Number(value & 0xFFn)); if (value < 128n) { return out } out[i] |= 0x80; value >>= 7n } }

class Serializer {
    constructor() { this.bytes = [] }
    finish = () => new Uint8Array(this.bytes)
    push_n = (bytes) => bytes.forEach((byte) => this.bytes.push(byte))
    serialize_bool = (value) => this.serialize_number(U8_BYTES, false, value ? 1 : 0)
    serialize_number = (n_bytes, signed, value) => { if (n_bytes === U8_BYTES) { this.bytes.push(new Uint8Array([value])[0]) } else if (n_bytes === U16_BYTES || n_bytes === U32_BYTES || n_bytes === U64_BYTES || n_bytes === U128_BYTES) { const value_b = BigInt(value), buffer = signed ? varint(n_bytes, zig_zag(n_bytes, value_b)) : varint(n_bytes, value_b); this.push_n(buffer) } else { throw "byte count not supported" } }
    serialize_number_float = (n_bytes, value) => { const b_buffer = new ArrayBuffer(n_bytes), b_view = new DataView(b_buffer); if (n_bytes === U32_BYTES) { b_view.setFloat32(0, value, true) } else if (n_bytes === U64_BYTES) { b_view.setFloat64(0, value, true) } else { throw "byte count not supported" } this.push_n(new Uint8Array(b_buffer)) }
    serialize_string = (str) => { const bytes = Array.from(new TextEncoder().encode(str)); this.push_n(varint(U32_BYTES, bytes.length)); this.push_n(bytes) }
    serialize_array = (ser, array, len) => { if (len == undefined) this.push_n(varint(U32_BYTES, array.length)); array.slice(0, len != undefined ? len : array.length).forEach((v) => ser(this, v)) }
    serialize_string_key_map = (ser, obj) => { const entries = Object.entries(obj); this.push_n(varint(U32_BYTES, entries.length)); entries.forEach(([i, v]) => { this.serialize_string(i); ser(this, v) }) }
    serialize_map = (ser, map) => { this.push_n(varint(U32_BYTES, map.size)); map.forEach((v, k) => ser(this, k, v)) }
}

class Deserializer {
    constructor(bytes_in) { this.bytes = Array.from(bytes_in); }
    pop_next = () => { const next = this.bytes.shift(); if (next === undefined) { throw "input buffer too small" } return next }
    pop_n = (n) => { const bytes = Array(); for (let i = 0; i < n; i++) { bytes.push(this.bytes.shift()) } return bytes }
    get_int8 = (signed) => signed ? new Int8Array([this.pop_next()])[0] : this.pop_next();
    try_take = (n_bytes) => { let out = 0n, v_max = varint_max(n_bytes); for (let i = 0; i < v_max; i++) { const val = this.pop_next(), carry = BigInt(val & 0x7F); out |= carry << BigInt(7 * i); if ((val & 0x80) === 0) { if (i === v_max - 1 && val > max_of_last_byte(n_bytes)) { throw "Bad Variant" } else return out } } throw "Bad Variant"; }
    deserialize_bool = () => { const byte = this.pop_next(); return byte === undefined ? undefined : byte > 0 ? true : false }
    deserialize_number = (n_bytes, signed) => { if (n_bytes === U8_BYTES) { return this.get_int8(signed) } else if (n_bytes === U16_BYTES || n_bytes === U32_BYTES || n_bytes === U64_BYTES || n_bytes === U128_BYTES) { const val = this.try_take(n_bytes); return to_number_if_safe(signed ? de_zig_zag_signed(val) : val) } else { throw "byte count not supported" } }
    deserialize_number_float = (n_bytes) => { const b_buffer = new ArrayBuffer(n_bytes), b_view = new DataView(b_buffer); this.pop_n(n_bytes).forEach((b, i) => b_view.setUint8(i, b)); if (n_bytes === U32_BYTES) { return b_view.getFloat32(0, true) } else if (n_bytes === U64_BYTES) { return b_view.getFloat64(0, true) } else { throw "byte count not supported" } }
    deserialize_string = () => { const bytes = this.pop_n(Number(this.try_take(U32_BYTES))); return new TextDecoder("utf-8").decode(Uint8Array.from(bytes)) }
    deserialize_array = (des, len) => Array.from({length: len === undefined ? Number(this.try_take(U32_BYTES)) : len}, (v, i) => des(this))
    deserialize_string_key_map = (des) => { return [...Array(Number(this.try_take(U32_BYTES)))].reduce((prev) => { prev[this.deserialize_string()] = des(this); return prev }, {}) }
    deserialize_map = (des) => { return [...Array(Number(this.try_take(U32_BYTES)))].reduce((prev) => { const d = des(this); prev.set(d[0], d[1]); return prev }, new Map()) }
    release_bytes = () => { return new Uint8Array(this.bytes); }
}

function check_bounds(v, n_bytes, signed, zero_able) {
    if (!zero_able && v === 0) {
        throw new Error("Value must not be zero")
    }
    const max = BigInt(2 ** (n_bytes * BITS_PER_BYTE)), value_b = BigInt(v);
    if (signed) {
        const bounds = max / 2n;
        if (value_b < -bounds || value_b >= bounds) {
            throw new Error("Value " + value_b + " is out of bounds (" + -bounds + ".." + bounds + ")")
        }
    } else {
        if (value_b >= max || value_b < 0) {
            throw new Error("Value " + value_b + " is out of bounds (0.." + max + ")")
        }
    }

    return true
}

function check_integer_type(v, n_bytes, signed, zero_able) {
    return (
        typeof v === "number" &&
        Number.isInteger(v) ||
        typeof v === "bigint"
    ) && check_bounds(v, n_bytes, signed, zero_able)
}

function is_APP_ICON(v) {
    return (typeof v === "object" &&
         "tag" in v &&
         v.tag === "Fader" ||
         v.tag === "AdEnv" ||
         v.tag === "Random" ||
         v.tag === "Euclid" ||
         v.tag === "Attenuate" ||
         v.tag === "Die" ||
         v.tag === "Quantize" ||
         v.tag === "Sequence" ||
         v.tag === "Note" ||
         v.tag === "EnvFollower" ||
         v.tag === "SoftRandom" ||
         v.tag === "Sine" ||
         v.tag === "NoteBox" ||
         v.tag === "SequenceSquare" ||
         v.tag === "NoteGrid" ||
         v.tag === "KnobRound" ||
         v.tag === "Stereo")
}

function is_AUX_JACK_MODE(v) {
    return (typeof v === "object" &&
         "tag" in v &&
         v.tag === "None" ||
         v.tag === "ResetOut") ||
         (typeof v === "object" &&
         "tag" in v &&
         "value" in v &&
         (v.tag === "ClockOut" &&
         is_CLOCK_DIVISION(v.value)))
}

function is_CLOCK_CONFIG(v) {
    return typeof v === "object" &&
         is_CLOCK_SRC(v.clock_src) &&
         check_integer_type(v.ext_ppqn, U8_BYTES, false, true) &&
         is_RESET_SRC(v.reset_src) &&
         typeof v.internal_bpm === "number" &&
         Number.isFinite(v.internal_bpm) &&
         check_integer_type(v.swing_amount, U8_BYTES, true, true);
}

function is_CLOCK_DIVISION(v) {
    return (typeof v === "object" &&
         "tag" in v &&
         v.tag === "_1" ||
         v.tag === "_2" ||
         v.tag === "_4" ||
         v.tag === "_6" ||
         v.tag === "_8" ||
         v.tag === "_12" ||
         v.tag === "_24" ||
         v.tag === "_96" ||
         v.tag === "_192" ||
         v.tag === "_384")
}

function is_CLOCK_SRC(v) {
    return (typeof v === "object" &&
         "tag" in v &&
         v.tag === "None" ||
         v.tag === "Atom" ||
         v.tag === "Meteor" ||
         v.tag === "Cube" ||
         v.tag === "Internal" ||
         v.tag === "MidiIn" ||
         v.tag === "MidiUsb")
}

function is_COLOR(v) {
    return (typeof v === "object" &&
         "tag" in v &&
         v.tag === "White" ||
         v.tag === "Yellow" ||
         v.tag === "Orange" ||
         v.tag === "Red" ||
         v.tag === "Lime" ||
         v.tag === "Green" ||
         v.tag === "Cyan" ||
         v.tag === "SkyBlue" ||
         v.tag === "Blue" ||
         v.tag === "Violet" ||
         v.tag === "Pink" ||
         v.tag === "PaleGreen" ||
         v.tag === "Sand" ||
         v.tag === "Rose" ||
         v.tag === "Salmon" ||
         v.tag === "LightBlue") ||
         (typeof v === "object" &&
         "tag" in v &&
         "value" in v &&
         (v.tag === "Custom" &&
         Array.isArray(v.value) &&
         v.value.length === 3 &&
         check_integer_type(v.value[0], U8_BYTES, false, true) &&
         check_integer_type(v.value[1], U8_BYTES, false, true) &&
         check_integer_type(v.value[2], U8_BYTES, false, true)))
}

function is_CONFIG_MSG_IN(v) {
    return (typeof v === "object" &&
         "tag" in v &&
         v.tag === "Ping" ||
         v.tag === "GetAllApps" ||
         v.tag === "GetGlobalConfig" ||
         v.tag === "GetLayout" ||
         v.tag === "GetAllAppParams" ||
         v.tag === "FactoryReset" ||
         v.tag === "GetVersion") ||
         (typeof v === "object" &&
         "tag" in v &&
         "value" in v &&
         (v.tag === "SetGlobalConfig" &&
         is_GLOBAL_CONFIG(v.value)) ||
         (v.tag === "SetLayout" &&
         is_LAYOUT(v.value)) ||
         (v.tag === "GetAppParams" &&
         typeof v.value === "object" &&
         check_integer_type(v.value.layout_id, U8_BYTES, false, true)) ||
         (v.tag === "SetAppParams" &&
         typeof v.value === "object" &&
         check_integer_type(v.value.layout_id, U8_BYTES, false, true) &&
         Array.isArray(v.value.values) &&
         v.value.values.every((v) => (v !== undefined &&
         is_VALUE(v)) ||
         v === undefined) &&
         v.value.values.length === 16))
}

function is_CONFIG_MSG_OUT(v) {
    return (typeof v === "object" &&
         "tag" in v &&
         v.tag === "Pong" ||
         v.tag === "BatchMsgEnd") ||
         (typeof v === "object" &&
         "tag" in v &&
         "value" in v &&
         (v.tag === "BatchMsgStart" &&
         check_integer_type(v.value, U64_BYTES, false, true)) ||
         (v.tag === "GlobalConfig" &&
         is_GLOBAL_CONFIG(v.value)) ||
         (v.tag === "Layout" &&
         is_LAYOUT(v.value)) ||
         (v.tag === "AppConfig" &&
         Array.isArray(v.value) &&
         v.value.length === 3 &&
         check_integer_type(v.value[0], U8_BYTES, false, true) &&
         check_integer_type(v.value[1], U64_BYTES, false, true) &&
         Array.isArray(v.value[2]) &&
         v.value[2].length === 6 &&
         check_integer_type(v.value[2][0], U64_BYTES, false, true) &&
         typeof v.value[2][1] === "string" &&
         typeof v.value[2][2] === "string" &&
         is_COLOR(v.value[2][3]) &&
         is_APP_ICON(v.value[2][4]) &&
         Array.isArray(v.value[2][5]) &&
         v.value[2][5].every((v) => is_PARAM(v))) ||
         (v.tag === "AppState" &&
         Array.isArray(v.value) &&
         v.value.length === 2 &&
         check_integer_type(v.value[0], U8_BYTES, false, true) &&
         Array.isArray(v.value[1]) &&
         v.value[1].every((v) => is_VALUE(v))) ||
         (v.tag === "Version" &&
         typeof v.value === "object" &&
         check_integer_type(v.value.major, U8_BYTES, false, true) &&
         check_integer_type(v.value.minor, U8_BYTES, false, true) &&
         check_integer_type(v.value.patch, U8_BYTES, false, true)))
}

function is_CURVE(v) {
    return (typeof v === "object" &&
         "tag" in v &&
         v.tag === "Linear" ||
         v.tag === "Logarithmic" ||
         v.tag === "Exponential")
}

function is_GLOBAL_CONFIG(v) {
    return typeof v === "object" &&
         Array.isArray(v.aux) &&
         v.aux.every((v) => is_AUX_JACK_MODE(v)) &&
         v.aux.length === 3 &&
         is_CLOCK_CONFIG(v.clock) &&
         is_I_2_C_MODE(v.i2c_mode) &&
         check_integer_type(v.led_brightness, U8_BYTES, false, true) &&
         is_MIDI_CONFIG(v.midi) &&
         is_QUANTIZER_CONFIG(v.quantizer) &&
         is_latch_TAKEOVER_MODE(v.takeover_mode);
}

function is_I_2_C_MODE(v) {
    return (typeof v === "object" &&
         "tag" in v &&
         v.tag === "Calibration" ||
         v.tag === "Leader" ||
         v.tag === "Follower")
}

function is_KEY(v) {
    return (typeof v === "object" &&
         "tag" in v &&
         v.tag === "Chromatic" ||
         v.tag === "Ionian" ||
         v.tag === "Dorian" ||
         v.tag === "Phrygian" ||
         v.tag === "Lydian" ||
         v.tag === "Mixolydian" ||
         v.tag === "Aeolian" ||
         v.tag === "Locrian" ||
         v.tag === "BluesMaj" ||
         v.tag === "BluesMin" ||
         v.tag === "PentatonicMaj" ||
         v.tag === "PentatonicMin" ||
         v.tag === "Folk" ||
         v.tag === "Japanese" ||
         v.tag === "Gamelan" ||
         v.tag === "HungarianMin" ||
         v.tag === "Off")
}

function is_LAYOUT(v) {
    return Array.isArray(v) &&
         v.length === 1 &&
         Array.isArray(v[0]) &&
         v[0].every((v) => (v !== undefined &&
         Array.isArray(v) &&
         v.length === 3 &&
         check_integer_type(v[0], U8_BYTES, false, true) &&
         check_integer_type(v[1], U64_BYTES, false, true) &&
         check_integer_type(v[2], U8_BYTES, false, true)) ||
         v === undefined) &&
         v[0].length === 16;
}

function is_MIDI_CC(v) {
    return Array.isArray(v) &&
         v.length === 1 &&
         check_integer_type(v[0], U16_BYTES, false, true);
}

function is_MIDI_CHANNEL(v) {
    return Array.isArray(v) &&
         v.length === 1 &&
         check_integer_type(v[0], U8_BYTES, false, true);
}

function is_MIDI_CONFIG(v) {
    return typeof v === "object" &&
         Array.isArray(v.outs) &&
         v.outs.every((v) => is_MIDI_OUT_CONFIG(v)) &&
         v.outs.length === 3;
}

function is_MIDI_IN(v) {
    return Array.isArray(v) &&
         v.length === 1 &&
         Array.isArray(v[0]) &&
         v[0].every((v) => typeof v === "boolean") &&
         v[0].length === 2;
}

function is_MIDI_MODE(v) {
    return (typeof v === "object" &&
         "tag" in v &&
         v.tag === "Note" ||
         v.tag === "Cc")
}

function is_MIDI_NOTE(v) {
    return Array.isArray(v) &&
         v.length === 1 &&
         check_integer_type(v[0], U8_BYTES, false, true);
}

function is_MIDI_OUT(v) {
    return Array.isArray(v) &&
         v.length === 1 &&
         Array.isArray(v[0]) &&
         v[0].every((v) => typeof v === "boolean") &&
         v[0].length === 3;
}

function is_MIDI_OUT_CONFIG(v) {
    return typeof v === "object" &&
         typeof v.send_clock === "boolean" &&
         typeof v.send_transport === "boolean" &&
         is_MIDI_OUT_MODE(v.mode);
}

function is_MIDI_OUT_MODE(v) {
    return (typeof v === "object" &&
         "tag" in v &&
         v.tag === "None" ||
         v.tag === "Local") ||
         (typeof v === "object" &&
         "tag" in v &&
         "value" in v &&
         (v.tag === "MidiThru" &&
         typeof v.value === "object" &&
         is_MIDI_IN(v.value.sources)) ||
         (v.tag === "MidiMerge" &&
         typeof v.value === "object" &&
         is_MIDI_IN(v.value.sources)))
}

function is_NOTE(v) {
    return (typeof v === "object" &&
         "tag" in v &&
         v.tag === "C" ||
         v.tag === "CSharp" ||
         v.tag === "D" ||
         v.tag === "DSharp" ||
         v.tag === "E" ||
         v.tag === "F" ||
         v.tag === "FSharp" ||
         v.tag === "G" ||
         v.tag === "GSharp" ||
         v.tag === "A" ||
         v.tag === "ASharp" ||
         v.tag === "B")
}

function is_PARAM(v) {
    return (typeof v === "object" &&
         "tag" in v &&
         v.tag === "None" ||
         v.tag === "MidiIn" ||
         v.tag === "MidiMode" ||
         v.tag === "MidiOut" ||
         v.tag === "MidiNrpn" ||
         v.tag === "VoltPerOct") ||
         (typeof v === "object" &&
         "tag" in v &&
         "value" in v &&
         (v.tag === "i32" &&
         typeof v.value === "object" &&
         typeof v.value.name === "string" &&
         check_integer_type(v.value.min, U32_BYTES, true, true) &&
         check_integer_type(v.value.max, U32_BYTES, true, true)) ||
         (v.tag === "f32" &&
         typeof v.value === "object" &&
         typeof v.value.name === "string" &&
         typeof v.value.min === "number" &&
         Number.isFinite(v.value.min) &&
         typeof v.value.max === "number" &&
         Number.isFinite(v.value.max)) ||
         (v.tag === "bool" &&
         typeof v.value === "object" &&
         typeof v.value.name === "string") ||
         (v.tag === "Enum" &&
         typeof v.value === "object" &&
         typeof v.value.name === "string" &&
         Array.isArray(v.value.variants) &&
         v.value.variants.every((v) => typeof v === "string")) ||
         (v.tag === "Curve" &&
         typeof v.value === "object" &&
         typeof v.value.name === "string" &&
         Array.isArray(v.value.variants) &&
         v.value.variants.every((v) => is_CURVE(v))) ||
         (v.tag === "Waveform" &&
         typeof v.value === "object" &&
         typeof v.value.name === "string" &&
         Array.isArray(v.value.variants) &&
         v.value.variants.every((v) => is_WAVEFORM(v))) ||
         (v.tag === "Color" &&
         typeof v.value === "object" &&
         typeof v.value.name === "string" &&
         Array.isArray(v.value.variants) &&
         v.value.variants.every((v) => is_COLOR(v))) ||
         (v.tag === "Range" &&
         typeof v.value === "object" &&
         typeof v.value.name === "string" &&
         Array.isArray(v.value.variants) &&
         v.value.variants.every((v) => is_RANGE(v))) ||
         (v.tag === "Note" &&
         typeof v.value === "object" &&
         typeof v.value.name === "string" &&
         Array.isArray(v.value.variants) &&
         v.value.variants.every((v) => is_NOTE(v))) ||
         (v.tag === "MidiCc" &&
         typeof v.value === "object" &&
         typeof v.value.name === "string") ||
         (v.tag === "MidiChannel" &&
         typeof v.value === "object" &&
         typeof v.value.name === "string") ||
         (v.tag === "MidiNote" &&
         typeof v.value === "object" &&
         typeof v.value.name === "string"))
}

function is_QUANTIZER_CONFIG(v) {
    return typeof v === "object" &&
         is_KEY(v.key) &&
         is_NOTE(v.tonic);
}

function is_RANGE(v) {
    return (typeof v === "object" &&
         "tag" in v &&
         v.tag === "_0_10V" ||
         v.tag === "_0_5V" ||
         v.tag === "_Neg5_5V")
}

function is_RESET_SRC(v) {
    return (typeof v === "object" &&
         "tag" in v &&
         v.tag === "None" ||
         v.tag === "Atom" ||
         v.tag === "Meteor" ||
         v.tag === "Cube")
}

function is_latch_TAKEOVER_MODE(v) {
    return (typeof v === "object" &&
         "tag" in v &&
         v.tag === "Pickup" ||
         v.tag === "Jump" ||
         v.tag === "Scale")
}

function is_VALUE(v) {
    return (typeof v === "object" &&
         "tag" in v &&
         "value" in v &&
         (v.tag === "i32" &&
         check_integer_type(v.value, U32_BYTES, true, true)) ||
         (v.tag === "f32" &&
         typeof v.value === "number" &&
         Number.isFinite(v.value)) ||
         (v.tag === "bool" &&
         typeof v.value === "boolean") ||
         (v.tag === "Enum" &&
         check_integer_type(v.value, U64_BYTES, false, true)) ||
         (v.tag === "Curve" &&
         is_CURVE(v.value)) ||
         (v.tag === "Waveform" &&
         is_WAVEFORM(v.value)) ||
         (v.tag === "Color" &&
         is_COLOR(v.value)) ||
         (v.tag === "Range" &&
         is_RANGE(v.value)) ||
         (v.tag === "Note" &&
         is_NOTE(v.value)) ||
         (v.tag === "MidiCc" &&
         is_MIDI_CC(v.value)) ||
         (v.tag === "MidiChannel" &&
         is_MIDI_CHANNEL(v.value)) ||
         (v.tag === "MidiIn" &&
         is_MIDI_IN(v.value)) ||
         (v.tag === "MidiMode" &&
         is_MIDI_MODE(v.value)) ||
         (v.tag === "MidiNote" &&
         is_MIDI_NOTE(v.value)) ||
         (v.tag === "MidiOut" &&
         is_MIDI_OUT(v.value)) ||
         (v.tag === "MidiNrpn" &&
         typeof v.value === "boolean") ||
         (v.tag === "VoltPerOct" &&
         is_VOLT_PER_OCT(v.value)))
}

function is_VOLT_PER_OCT(v) {
    return (typeof v === "object" &&
         "tag" in v &&
         v.tag === "Standard" ||
         v.tag === "Buchla")
}

function is_WAVEFORM(v) {
    return (typeof v === "object" &&
         "tag" in v &&
         v.tag === "Triangle" ||
         v.tag === "Saw" ||
         v.tag === "SawInv" ||
         v.tag === "Square" ||
         v.tag === "Sine")
}

function serialize_APP_ICON(s, v) {
    switch (v.tag) {
    case "Fader":
        s.serialize_number(U32_BYTES, false, 0);
        break;
    case "AdEnv":
        s.serialize_number(U32_BYTES, false, 1);
        break;
    case "Random":
        s.serialize_number(U32_BYTES, false, 2);
        break;
    case "Euclid":
        s.serialize_number(U32_BYTES, false, 3);
        break;
    case "Attenuate":
        s.serialize_number(U32_BYTES, false, 4);
        break;
    case "Die":
        s.serialize_number(U32_BYTES, false, 5);
        break;
    case "Quantize":
        s.serialize_number(U32_BYTES, false, 6);
        break;
    case "Sequence":
        s.serialize_number(U32_BYTES, false, 7);
        break;
    case "Note":
        s.serialize_number(U32_BYTES, false, 8);
        break;
    case "EnvFollower":
        s.serialize_number(U32_BYTES, false, 9);
        break;
    case "SoftRandom":
        s.serialize_number(U32_BYTES, false, 10);
        break;
    case "Sine":
        s.serialize_number(U32_BYTES, false, 11);
        break;
    case "NoteBox":
        s.serialize_number(U32_BYTES, false, 12);
        break;
    case "SequenceSquare":
        s.serialize_number(U32_BYTES, false, 13);
        break;
    case "NoteGrid":
        s.serialize_number(U32_BYTES, false, 14);
        break;
    case "KnobRound":
        s.serialize_number(U32_BYTES, false, 15);
        break;
    case "Stereo":
        s.serialize_number(U32_BYTES, false, 16);
        break;
    default:
        throw "variant not implemented"
    }
}

function serialize_AUX_JACK_MODE(s, v) {
    switch (v.tag) {
    case "None":
        s.serialize_number(U32_BYTES, false, 0);
        break;
    case "ClockOut":
        s.serialize_number(U32_BYTES, false, 1);
        serialize_CLOCK_DIVISION(s, v.value);
        break;
    case "ResetOut":
        s.serialize_number(U32_BYTES, false, 2);
        break;
    default:
        throw "variant not implemented"
    }
}

function serialize_CLOCK_CONFIG(s, v) {
    serialize_CLOCK_SRC(s, v.clock_src);
    s.serialize_number(U8_BYTES, false, v.ext_ppqn);
    serialize_RESET_SRC(s, v.reset_src);
    s.serialize_number_float(U32_BYTES, v.internal_bpm);
    s.serialize_number(U8_BYTES, true, v.swing_amount);
}

function serialize_CLOCK_DIVISION(s, v) {
    switch (v.tag) {
    case "_1":
        s.serialize_number(U32_BYTES, false, 0);
        break;
    case "_2":
        s.serialize_number(U32_BYTES, false, 1);
        break;
    case "_4":
        s.serialize_number(U32_BYTES, false, 2);
        break;
    case "_6":
        s.serialize_number(U32_BYTES, false, 3);
        break;
    case "_8":
        s.serialize_number(U32_BYTES, false, 4);
        break;
    case "_12":
        s.serialize_number(U32_BYTES, false, 5);
        break;
    case "_24":
        s.serialize_number(U32_BYTES, false, 6);
        break;
    case "_96":
        s.serialize_number(U32_BYTES, false, 7);
        break;
    case "_192":
        s.serialize_number(U32_BYTES, false, 8);
        break;
    case "_384":
        s.serialize_number(U32_BYTES, false, 9);
        break;
    default:
        throw "variant not implemented"
    }
}

function serialize_CLOCK_SRC(s, v) {
    switch (v.tag) {
    case "None":
        s.serialize_number(U32_BYTES, false, 0);
        break;
    case "Atom":
        s.serialize_number(U32_BYTES, false, 1);
        break;
    case "Meteor":
        s.serialize_number(U32_BYTES, false, 2);
        break;
    case "Cube":
        s.serialize_number(U32_BYTES, false, 3);
        break;
    case "Internal":
        s.serialize_number(U32_BYTES, false, 4);
        break;
    case "MidiIn":
        s.serialize_number(U32_BYTES, false, 5);
        break;
    case "MidiUsb":
        s.serialize_number(U32_BYTES, false, 6);
        break;
    default:
        throw "variant not implemented"
    }
}

function serialize_COLOR(s, v) {
    switch (v.tag) {
    case "White":
        s.serialize_number(U32_BYTES, false, 0);
        break;
    case "Yellow":
        s.serialize_number(U32_BYTES, false, 1);
        break;
    case "Orange":
        s.serialize_number(U32_BYTES, false, 2);
        break;
    case "Red":
        s.serialize_number(U32_BYTES, false, 3);
        break;
    case "Lime":
        s.serialize_number(U32_BYTES, false, 4);
        break;
    case "Green":
        s.serialize_number(U32_BYTES, false, 5);
        break;
    case "Cyan":
        s.serialize_number(U32_BYTES, false, 6);
        break;
    case "SkyBlue":
        s.serialize_number(U32_BYTES, false, 7);
        break;
    case "Blue":
        s.serialize_number(U32_BYTES, false, 8);
        break;
    case "Violet":
        s.serialize_number(U32_BYTES, false, 9);
        break;
    case "Pink":
        s.serialize_number(U32_BYTES, false, 10);
        break;
    case "PaleGreen":
        s.serialize_number(U32_BYTES, false, 11);
        break;
    case "Sand":
        s.serialize_number(U32_BYTES, false, 12);
        break;
    case "Rose":
        s.serialize_number(U32_BYTES, false, 13);
        break;
    case "Salmon":
        s.serialize_number(U32_BYTES, false, 14);
        break;
    case "LightBlue":
        s.serialize_number(U32_BYTES, false, 15);
        break;
    case "Custom":
        {
            s.serialize_number(U32_BYTES, false, 16);
            s.serialize_number(U8_BYTES, false, v.value[0]);
            s.serialize_number(U8_BYTES, false, v.value[1]);
            s.serialize_number(U8_BYTES, false, v.value[2]);
        }
        break;
    default:
        throw "variant not implemented"
    }
}

function serialize_CONFIG_MSG_IN(s, v) {
    switch (v.tag) {
    case "Ping":
        s.serialize_number(U32_BYTES, false, 0);
        break;
    case "GetAllApps":
        s.serialize_number(U32_BYTES, false, 1);
        break;
    case "GetGlobalConfig":
        s.serialize_number(U32_BYTES, false, 2);
        break;
    case "SetGlobalConfig":
        s.serialize_number(U32_BYTES, false, 3);
        serialize_GLOBAL_CONFIG(s, v.value);
        break;
    case "GetLayout":
        s.serialize_number(U32_BYTES, false, 4);
        break;
    case "SetLayout":
        s.serialize_number(U32_BYTES, false, 5);
        serialize_LAYOUT(s, v.value);
        break;
    case "GetAllAppParams":
        s.serialize_number(U32_BYTES, false, 6);
        break;
    case "GetAppParams":
        s.serialize_number(U32_BYTES, false, 7);
        s.serialize_number(U8_BYTES, false, v.value.layout_id);
        break;
    case "SetAppParams":
        {
            s.serialize_number(U32_BYTES, false, 8);
            s.serialize_number(U8_BYTES, false, v.value.layout_id);
            const lambda_v_value_values = (s, v) => {
                if (v !== undefined) {
                    s.serialize_number(U32_BYTES, false, 1);
                    serialize_VALUE(s, v)
                } else {
                    s.serialize_number(U32_BYTES, false, 0)
                }
            };
            s.serialize_array(lambda_v_value_values, v.value.values, 16);
        }
        break;
    case "FactoryReset":
        s.serialize_number(U32_BYTES, false, 9);
        break;
    case "GetVersion":
        s.serialize_number(U32_BYTES, false, 10);
        break;
    default:
        throw "variant not implemented"
    }
}

function serialize_CONFIG_MSG_OUT(s, v) {
    switch (v.tag) {
    case "Pong":
        s.serialize_number(U32_BYTES, false, 0);
        break;
    case "BatchMsgStart":
        s.serialize_number(U32_BYTES, false, 1);
        s.serialize_number(U64_BYTES, false, v.value);
        break;
    case "BatchMsgEnd":
        s.serialize_number(U32_BYTES, false, 2);
        break;
    case "GlobalConfig":
        s.serialize_number(U32_BYTES, false, 3);
        serialize_GLOBAL_CONFIG(s, v.value);
        break;
    case "Layout":
        s.serialize_number(U32_BYTES, false, 4);
        serialize_LAYOUT(s, v.value);
        break;
    case "AppConfig":
        {
            s.serialize_number(U32_BYTES, false, 5);
            s.serialize_number(U8_BYTES, false, v.value[0]);
            s.serialize_number(U64_BYTES, false, v.value[1]);
            s.serialize_number(U64_BYTES, false, v.value[2][0]);
            s.serialize_string(v.value[2][1]);
            s.serialize_string(v.value[2][2]);
            serialize_COLOR(s, v.value[2][3]);
            serialize_APP_ICON(s, v.value[2][4]);
            const lambda_v_value_2_5 = (s, v) => {
                serialize_PARAM(s, v)
            };
            s.serialize_array(lambda_v_value_2_5, v.value[2][5]);
        }
        break;
    case "AppState":
        {
            s.serialize_number(U32_BYTES, false, 6);
            s.serialize_number(U8_BYTES, false, v.value[0]);
            const lambda_v_value_1 = (s, v) => {
                serialize_VALUE(s, v)
            };
            s.serialize_array(lambda_v_value_1, v.value[1]);
        }
        break;
    case "Version":
        {
            s.serialize_number(U32_BYTES, false, 7);
            s.serialize_number(U8_BYTES, false, v.value.major);
            s.serialize_number(U8_BYTES, false, v.value.minor);
            s.serialize_number(U8_BYTES, false, v.value.patch);
        }
        break;
    default:
        throw "variant not implemented"
    }
}

function serialize_CURVE(s, v) {
    switch (v.tag) {
    case "Linear":
        s.serialize_number(U32_BYTES, false, 0);
        break;
    case "Logarithmic":
        s.serialize_number(U32_BYTES, false, 1);
        break;
    case "Exponential":
        s.serialize_number(U32_BYTES, false, 2);
        break;
    default:
        throw "variant not implemented"
    }
}

function serialize_GLOBAL_CONFIG(s, v) {
    const lambda_v_aux = (s, v) => {
        serialize_AUX_JACK_MODE(s, v)
    };
    s.serialize_array(lambda_v_aux, v.aux, 3);
    serialize_CLOCK_CONFIG(s, v.clock);
    serialize_I_2_C_MODE(s, v.i2c_mode);
    s.serialize_number(U8_BYTES, false, v.led_brightness);
    serialize_MIDI_CONFIG(s, v.midi);
    serialize_QUANTIZER_CONFIG(s, v.quantizer);
    serialize_latch_TAKEOVER_MODE(s, v.takeover_mode);
}

function serialize_I_2_C_MODE(s, v) {
    switch (v.tag) {
    case "Calibration":
        s.serialize_number(U32_BYTES, false, 0);
        break;
    case "Leader":
        s.serialize_number(U32_BYTES, false, 1);
        break;
    case "Follower":
        s.serialize_number(U32_BYTES, false, 2);
        break;
    default:
        throw "variant not implemented"
    }
}

function serialize_KEY(s, v) {
    switch (v.tag) {
    case "Chromatic":
        s.serialize_number(U32_BYTES, false, 0);
        break;
    case "Ionian":
        s.serialize_number(U32_BYTES, false, 1);
        break;
    case "Dorian":
        s.serialize_number(U32_BYTES, false, 2);
        break;
    case "Phrygian":
        s.serialize_number(U32_BYTES, false, 3);
        break;
    case "Lydian":
        s.serialize_number(U32_BYTES, false, 4);
        break;
    case "Mixolydian":
        s.serialize_number(U32_BYTES, false, 5);
        break;
    case "Aeolian":
        s.serialize_number(U32_BYTES, false, 6);
        break;
    case "Locrian":
        s.serialize_number(U32_BYTES, false, 7);
        break;
    case "BluesMaj":
        s.serialize_number(U32_BYTES, false, 8);
        break;
    case "BluesMin":
        s.serialize_number(U32_BYTES, false, 9);
        break;
    case "PentatonicMaj":
        s.serialize_number(U32_BYTES, false, 10);
        break;
    case "PentatonicMin":
        s.serialize_number(U32_BYTES, false, 11);
        break;
    case "Folk":
        s.serialize_number(U32_BYTES, false, 12);
        break;
    case "Japanese":
        s.serialize_number(U32_BYTES, false, 13);
        break;
    case "Gamelan":
        s.serialize_number(U32_BYTES, false, 14);
        break;
    case "HungarianMin":
        s.serialize_number(U32_BYTES, false, 15);
        break;
    case "Off":
        s.serialize_number(U32_BYTES, false, 16);
        break;
    default:
        throw "variant not implemented"
    }
}

function serialize_LAYOUT(s, v) {
    const lambda_v_0 = (s, v) => {
        if (v !== undefined) {
            s.serialize_number(U32_BYTES, false, 1);
            s.serialize_number(U8_BYTES, false, v[0]);
            s.serialize_number(U64_BYTES, false, v[1]);
            s.serialize_number(U8_BYTES, false, v[2])
        } else {
            s.serialize_number(U32_BYTES, false, 0)
        }
    };
    s.serialize_array(lambda_v_0, v[0], 16);
}

function serialize_MIDI_CC(s, v) {
    s.serialize_number(U16_BYTES, false, v[0]);
}

function serialize_MIDI_CHANNEL(s, v) {
    s.serialize_number(U8_BYTES, false, v[0]);
}

function serialize_MIDI_CONFIG(s, v) {
    const lambda_v_outs = (s, v) => {
        serialize_MIDI_OUT_CONFIG(s, v)
    };
    s.serialize_array(lambda_v_outs, v.outs, 3);
}

function serialize_MIDI_IN(s, v) {
    const lambda_v_0 = (s, v) => {
        s.serialize_bool(v)
    };
    s.serialize_array(lambda_v_0, v[0], 2);
}

function serialize_MIDI_MODE(s, v) {
    switch (v.tag) {
    case "Note":
        s.serialize_number(U32_BYTES, false, 0);
        break;
    case "Cc":
        s.serialize_number(U32_BYTES, false, 1);
        break;
    default:
        throw "variant not implemented"
    }
}

function serialize_MIDI_NOTE(s, v) {
    s.serialize_number(U8_BYTES, false, v[0]);
}

function serialize_MIDI_OUT(s, v) {
    const lambda_v_0 = (s, v) => {
        s.serialize_bool(v)
    };
    s.serialize_array(lambda_v_0, v[0], 3);
}

function serialize_MIDI_OUT_CONFIG(s, v) {
    s.serialize_bool(v.send_clock);
    s.serialize_bool(v.send_transport);
    serialize_MIDI_OUT_MODE(s, v.mode);
}

function serialize_MIDI_OUT_MODE(s, v) {
    switch (v.tag) {
    case "None":
        s.serialize_number(U32_BYTES, false, 0);
        break;
    case "Local":
        s.serialize_number(U32_BYTES, false, 1);
        break;
    case "MidiThru":
        s.serialize_number(U32_BYTES, false, 2);
        serialize_MIDI_IN(s, v.value.sources);
        break;
    case "MidiMerge":
        s.serialize_number(U32_BYTES, false, 3);
        serialize_MIDI_IN(s, v.value.sources);
        break;
    default:
        throw "variant not implemented"
    }
}

function serialize_NOTE(s, v) {
    switch (v.tag) {
    case "C":
        s.serialize_number(U32_BYTES, false, 0);
        break;
    case "CSharp":
        s.serialize_number(U32_BYTES, false, 1);
        break;
    case "D":
        s.serialize_number(U32_BYTES, false, 2);
        break;
    case "DSharp":
        s.serialize_number(U32_BYTES, false, 3);
        break;
    case "E":
        s.serialize_number(U32_BYTES, false, 4);
        break;
    case "F":
        s.serialize_number(U32_BYTES, false, 5);
        break;
    case "FSharp":
        s.serialize_number(U32_BYTES, false, 6);
        break;
    case "G":
        s.serialize_number(U32_BYTES, false, 7);
        break;
    case "GSharp":
        s.serialize_number(U32_BYTES, false, 8);
        break;
    case "A":
        s.serialize_number(U32_BYTES, false, 9);
        break;
    case "ASharp":
        s.serialize_number(U32_BYTES, false, 10);
        break;
    case "B":
        s.serialize_number(U32_BYTES, false, 11);
        break;
    default:
        throw "variant not implemented"
    }
}

function serialize_PARAM(s, v) {
    switch (v.tag) {
    case "None":
        s.serialize_number(U32_BYTES, false, 0);
        break;
    case "i32":
        {
            s.serialize_number(U32_BYTES, false, 1);
            s.serialize_string(v.value.name);
            s.serialize_number(U32_BYTES, true, v.value.min);
            s.serialize_number(U32_BYTES, true, v.value.max);
        }
        break;
    case "f32":
        {
            s.serialize_number(U32_BYTES, false, 2);
            s.serialize_string(v.value.name);
            s.serialize_number_float(U32_BYTES, v.value.min);
            s.serialize_number_float(U32_BYTES, v.value.max);
        }
        break;
    case "bool":
        s.serialize_number(U32_BYTES, false, 3);
        s.serialize_string(v.value.name);
        break;
    case "Enum":
        {
            s.serialize_number(U32_BYTES, false, 4);
            s.serialize_string(v.value.name);
            const lambda_v_value_variants = (s, v) => {
                s.serialize_string(v)
            };
            s.serialize_array(lambda_v_value_variants, v.value.variants);
        }
        break;
    case "Curve":
        {
            s.serialize_number(U32_BYTES, false, 5);
            s.serialize_string(v.value.name);
            const lambda_v_value_variants = (s, v) => {
                serialize_CURVE(s, v)
            };
            s.serialize_array(lambda_v_value_variants, v.value.variants);
        }
        break;
    case "Waveform":
        {
            s.serialize_number(U32_BYTES, false, 6);
            s.serialize_string(v.value.name);
            const lambda_v_value_variants = (s, v) => {
                serialize_WAVEFORM(s, v)
            };
            s.serialize_array(lambda_v_value_variants, v.value.variants);
        }
        break;
    case "Color":
        {
            s.serialize_number(U32_BYTES, false, 7);
            s.serialize_string(v.value.name);
            const lambda_v_value_variants = (s, v) => {
                serialize_COLOR(s, v)
            };
            s.serialize_array(lambda_v_value_variants, v.value.variants);
        }
        break;
    case "Range":
        {
            s.serialize_number(U32_BYTES, false, 8);
            s.serialize_string(v.value.name);
            const lambda_v_value_variants = (s, v) => {
                serialize_RANGE(s, v)
            };
            s.serialize_array(lambda_v_value_variants, v.value.variants);
        }
        break;
    case "Note":
        {
            s.serialize_number(U32_BYTES, false, 9);
            s.serialize_string(v.value.name);
            const lambda_v_value_variants = (s, v) => {
                serialize_NOTE(s, v)
            };
            s.serialize_array(lambda_v_value_variants, v.value.variants);
        }
        break;
    case "MidiCc":
        s.serialize_number(U32_BYTES, false, 10);
        s.serialize_string(v.value.name);
        break;
    case "MidiChannel":
        s.serialize_number(U32_BYTES, false, 11);
        s.serialize_string(v.value.name);
        break;
    case "MidiIn":
        s.serialize_number(U32_BYTES, false, 12);
        break;
    case "MidiMode":
        s.serialize_number(U32_BYTES, false, 13);
        break;
    case "MidiNote":
        s.serialize_number(U32_BYTES, false, 14);
        s.serialize_string(v.value.name);
        break;
    case "MidiOut":
        s.serialize_number(U32_BYTES, false, 15);
        break;
    case "MidiNrpn":
        s.serialize_number(U32_BYTES, false, 16);
        break;
    case "VoltPerOct":
        s.serialize_number(U32_BYTES, false, 17);
        break;
    default:
        throw "variant not implemented"
    }
}

function serialize_QUANTIZER_CONFIG(s, v) {
    serialize_KEY(s, v.key);
    serialize_NOTE(s, v.tonic);
}

function serialize_RANGE(s, v) {
    switch (v.tag) {
    case "_0_10V":
        s.serialize_number(U32_BYTES, false, 0);
        break;
    case "_0_5V":
        s.serialize_number(U32_BYTES, false, 1);
        break;
    case "_Neg5_5V":
        s.serialize_number(U32_BYTES, false, 2);
        break;
    default:
        throw "variant not implemented"
    }
}

function serialize_RESET_SRC(s, v) {
    switch (v.tag) {
    case "None":
        s.serialize_number(U32_BYTES, false, 0);
        break;
    case "Atom":
        s.serialize_number(U32_BYTES, false, 1);
        break;
    case "Meteor":
        s.serialize_number(U32_BYTES, false, 2);
        break;
    case "Cube":
        s.serialize_number(U32_BYTES, false, 3);
        break;
    default:
        throw "variant not implemented"
    }
}

function serialize_latch_TAKEOVER_MODE(s, v) {
    switch (v.tag) {
    case "Pickup":
        s.serialize_number(U32_BYTES, false, 0);
        break;
    case "Jump":
        s.serialize_number(U32_BYTES, false, 1);
        break;
    case "Scale":
        s.serialize_number(U32_BYTES, false, 2);
        break;
    default:
        throw "variant not implemented"
    }
}

function serialize_VALUE(s, v) {
    switch (v.tag) {
    case "i32":
        s.serialize_number(U32_BYTES, false, 0);
        s.serialize_number(U32_BYTES, true, v.value);
        break;
    case "f32":
        s.serialize_number(U32_BYTES, false, 1);
        s.serialize_number_float(U32_BYTES, v.value);
        break;
    case "bool":
        s.serialize_number(U32_BYTES, false, 2);
        s.serialize_bool(v.value);
        break;
    case "Enum":
        s.serialize_number(U32_BYTES, false, 3);
        s.serialize_number(U64_BYTES, false, v.value);
        break;
    case "Curve":
        s.serialize_number(U32_BYTES, false, 4);
        serialize_CURVE(s, v.value);
        break;
    case "Waveform":
        s.serialize_number(U32_BYTES, false, 5);
        serialize_WAVEFORM(s, v.value);
        break;
    case "Color":
        s.serialize_number(U32_BYTES, false, 6);
        serialize_COLOR(s, v.value);
        break;
    case "Range":
        s.serialize_number(U32_BYTES, false, 7);
        serialize_RANGE(s, v.value);
        break;
    case "Note":
        s.serialize_number(U32_BYTES, false, 8);
        serialize_NOTE(s, v.value);
        break;
    case "MidiCc":
        s.serialize_number(U32_BYTES, false, 9);
        serialize_MIDI_CC(s, v.value);
        break;
    case "MidiChannel":
        s.serialize_number(U32_BYTES, false, 10);
        serialize_MIDI_CHANNEL(s, v.value);
        break;
    case "MidiIn":
        s.serialize_number(U32_BYTES, false, 11);
        serialize_MIDI_IN(s, v.value);
        break;
    case "MidiMode":
        s.serialize_number(U32_BYTES, false, 12);
        serialize_MIDI_MODE(s, v.value);
        break;
    case "MidiNote":
        s.serialize_number(U32_BYTES, false, 13);
        serialize_MIDI_NOTE(s, v.value);
        break;
    case "MidiOut":
        s.serialize_number(U32_BYTES, false, 14);
        serialize_MIDI_OUT(s, v.value);
        break;
    case "MidiNrpn":
        s.serialize_number(U32_BYTES, false, 15);
        s.serialize_bool(v.value);
        break;
    case "VoltPerOct":
        s.serialize_number(U32_BYTES, false, 16);
        serialize_VOLT_PER_OCT(s, v.value);
        break;
    default:
        throw "variant not implemented"
    }
}

function serialize_VOLT_PER_OCT(s, v) {
    switch (v.tag) {
    case "Standard":
        s.serialize_number(U32_BYTES, false, 0);
        break;
    case "Buchla":
        s.serialize_number(U32_BYTES, false, 1);
        break;
    default:
        throw "variant not implemented"
    }
}

function serialize_WAVEFORM(s, v) {
    switch (v.tag) {
    case "Triangle":
        s.serialize_number(U32_BYTES, false, 0);
        break;
    case "Saw":
        s.serialize_number(U32_BYTES, false, 1);
        break;
    case "SawInv":
        s.serialize_number(U32_BYTES, false, 2);
        break;
    case "Square":
        s.serialize_number(U32_BYTES, false, 3);
        break;
    case "Sine":
        s.serialize_number(U32_BYTES, false, 4);
        break;
    default:
        throw "variant not implemented"
    }
}

/**
 * Serialize a value to an array of bytes.
 * @param {string} type - The type of the value to serialize.
 * @param {Object} value - The value to serialize.
 * @return {Uint8Array} The serialized value as an array of bytes.
 */
function serialize(type, value) {
    if (!(typeof type === "string")) {
        throw new Error("type must be a string");
    }
    const s = new Serializer();
    switch (type) {
    case "AppIcon":
        if (is_APP_ICON(value)) {
            serialize_APP_ICON(s, value);
        } else {
            throw new Error("Value " + "AppIcon" + " has wrong format");
        }
        break;
    case "AuxJackMode":
        if (is_AUX_JACK_MODE(value)) {
            serialize_AUX_JACK_MODE(s, value);
        } else {
            throw new Error("Value " + "AuxJackMode" + " has wrong format");
        }
        break;
    case "ClockConfig":
        if (is_CLOCK_CONFIG(value)) {
            serialize_CLOCK_CONFIG(s, value);
        } else {
            throw new Error("Value " + "ClockConfig" + " has wrong format");
        }
        break;
    case "ClockDivision":
        if (is_CLOCK_DIVISION(value)) {
            serialize_CLOCK_DIVISION(s, value);
        } else {
            throw new Error("Value " + "ClockDivision" + " has wrong format");
        }
        break;
    case "ClockSrc":
        if (is_CLOCK_SRC(value)) {
            serialize_CLOCK_SRC(s, value);
        } else {
            throw new Error("Value " + "ClockSrc" + " has wrong format");
        }
        break;
    case "Color":
        if (is_COLOR(value)) {
            serialize_COLOR(s, value);
        } else {
            throw new Error("Value " + "Color" + " has wrong format");
        }
        break;
    case "ConfigMsgIn":
        if (is_CONFIG_MSG_IN(value)) {
            serialize_CONFIG_MSG_IN(s, value);
        } else {
            throw new Error("Value " + "ConfigMsgIn" + " has wrong format");
        }
        break;
    case "ConfigMsgOut":
        if (is_CONFIG_MSG_OUT(value)) {
            serialize_CONFIG_MSG_OUT(s, value);
        } else {
            throw new Error("Value " + "ConfigMsgOut" + " has wrong format");
        }
        break;
    case "Curve":
        if (is_CURVE(value)) {
            serialize_CURVE(s, value);
        } else {
            throw new Error("Value " + "Curve" + " has wrong format");
        }
        break;
    case "GlobalConfig":
        if (is_GLOBAL_CONFIG(value)) {
            serialize_GLOBAL_CONFIG(s, value);
        } else {
            throw new Error("Value " + "GlobalConfig" + " has wrong format");
        }
        break;
    case "I2cMode":
        if (is_I_2_C_MODE(value)) {
            serialize_I_2_C_MODE(s, value);
        } else {
            throw new Error("Value " + "I2cMode" + " has wrong format");
        }
        break;
    case "Key":
        if (is_KEY(value)) {
            serialize_KEY(s, value);
        } else {
            throw new Error("Value " + "Key" + " has wrong format");
        }
        break;
    case "Layout":
        if (is_LAYOUT(value)) {
            serialize_LAYOUT(s, value);
        } else {
            throw new Error("Value " + "Layout" + " has wrong format");
        }
        break;
    case "MidiCc":
        if (is_MIDI_CC(value)) {
            serialize_MIDI_CC(s, value);
        } else {
            throw new Error("Value " + "MidiCc" + " has wrong format");
        }
        break;
    case "MidiChannel":
        if (is_MIDI_CHANNEL(value)) {
            serialize_MIDI_CHANNEL(s, value);
        } else {
            throw new Error("Value " + "MidiChannel" + " has wrong format");
        }
        break;
    case "MidiConfig":
        if (is_MIDI_CONFIG(value)) {
            serialize_MIDI_CONFIG(s, value);
        } else {
            throw new Error("Value " + "MidiConfig" + " has wrong format");
        }
        break;
    case "MidiIn":
        if (is_MIDI_IN(value)) {
            serialize_MIDI_IN(s, value);
        } else {
            throw new Error("Value " + "MidiIn" + " has wrong format");
        }
        break;
    case "MidiMode":
        if (is_MIDI_MODE(value)) {
            serialize_MIDI_MODE(s, value);
        } else {
            throw new Error("Value " + "MidiMode" + " has wrong format");
        }
        break;
    case "MidiNote":
        if (is_MIDI_NOTE(value)) {
            serialize_MIDI_NOTE(s, value);
        } else {
            throw new Error("Value " + "MidiNote" + " has wrong format");
        }
        break;
    case "MidiOut":
        if (is_MIDI_OUT(value)) {
            serialize_MIDI_OUT(s, value);
        } else {
            throw new Error("Value " + "MidiOut" + " has wrong format");
        }
        break;
    case "MidiOutConfig":
        if (is_MIDI_OUT_CONFIG(value)) {
            serialize_MIDI_OUT_CONFIG(s, value);
        } else {
            throw new Error("Value " + "MidiOutConfig" + " has wrong format");
        }
        break;
    case "MidiOutMode":
        if (is_MIDI_OUT_MODE(value)) {
            serialize_MIDI_OUT_MODE(s, value);
        } else {
            throw new Error("Value " + "MidiOutMode" + " has wrong format");
        }
        break;
    case "Note":
        if (is_NOTE(value)) {
            serialize_NOTE(s, value);
        } else {
            throw new Error("Value " + "Note" + " has wrong format");
        }
        break;
    case "Param":
        if (is_PARAM(value)) {
            serialize_PARAM(s, value);
        } else {
            throw new Error("Value " + "Param" + " has wrong format");
        }
        break;
    case "QuantizerConfig":
        if (is_QUANTIZER_CONFIG(value)) {
            serialize_QUANTIZER_CONFIG(s, value);
        } else {
            throw new Error("Value " + "QuantizerConfig" + " has wrong format");
        }
        break;
    case "Range":
        if (is_RANGE(value)) {
            serialize_RANGE(s, value);
        } else {
            throw new Error("Value " + "Range" + " has wrong format");
        }
        break;
    case "ResetSrc":
        if (is_RESET_SRC(value)) {
            serialize_RESET_SRC(s, value);
        } else {
            throw new Error("Value " + "ResetSrc" + " has wrong format");
        }
        break;
    case "latch.TakeoverMode":
        if (is_latch_TAKEOVER_MODE(value)) {
            serialize_latch_TAKEOVER_MODE(s, value);
        } else {
            throw new Error("Value " + "latch.TakeoverMode" + " has wrong format");
        }
        break;
    case "Value":
        if (is_VALUE(value)) {
            serialize_VALUE(s, value);
        } else {
            throw new Error("Value " + "Value" + " has wrong format");
        }
        break;
    case "VoltPerOct":
        if (is_VOLT_PER_OCT(value)) {
            serialize_VOLT_PER_OCT(s, value);
        } else {
            throw new Error("Value " + "VoltPerOct" + " has wrong format");
        }
        break;
    case "Waveform":
        if (is_WAVEFORM(value)) {
            serialize_WAVEFORM(s, value);
        } else {
            throw new Error("Value " + "Waveform" + " has wrong format");
        }
        break;
    default:
        throw "type not implemented";
    }
    return s.finish();
}

export {
    serialize
};

function deserialize_APP_ICON(d) {
    switch (d.deserialize_number(U32_BYTES, false)) {
    case 0:
        return {
            tag: "Fader"
        };
    case 1:
        return {
            tag: "AdEnv"
        };
    case 2:
        return {
            tag: "Random"
        };
    case 3:
        return {
            tag: "Euclid"
        };
    case 4:
        return {
            tag: "Attenuate"
        };
    case 5:
        return {
            tag: "Die"
        };
    case 6:
        return {
            tag: "Quantize"
        };
    case 7:
        return {
            tag: "Sequence"
        };
    case 8:
        return {
            tag: "Note"
        };
    case 9:
        return {
            tag: "EnvFollower"
        };
    case 10:
        return {
            tag: "SoftRandom"
        };
    case 11:
        return {
            tag: "Sine"
        };
    case 12:
        return {
            tag: "NoteBox"
        };
    case 13:
        return {
            tag: "SequenceSquare"
        };
    case 14:
        return {
            tag: "NoteGrid"
        };
    case 15:
        return {
            tag: "KnobRound"
        };
    case 16:
        return {
            tag: "Stereo"
        };
    default:
        throw "variant not implemented"
    }
}

function deserialize_AUX_JACK_MODE(d) {
    switch (d.deserialize_number(U32_BYTES, false)) {
    case 0:
        return {
            tag: "None"
        };
    case 1:
        return {
            tag: "ClockOut",
            value: deserialize_CLOCK_DIVISION(d)
        };
    case 2:
        return {
            tag: "ResetOut"
        };
    default:
        throw "variant not implemented"
    }
}

function deserialize_CLOCK_CONFIG(d) {
    return {
        clock_src: deserialize_CLOCK_SRC(d),
        ext_ppqn: d.deserialize_number(U8_BYTES, false),
        reset_src: deserialize_RESET_SRC(d),
        internal_bpm: d.deserialize_number_float(U32_BYTES),
        swing_amount: d.deserialize_number(U8_BYTES, true)
    };
}

function deserialize_CLOCK_DIVISION(d) {
    switch (d.deserialize_number(U32_BYTES, false)) {
    case 0:
        return {
            tag: "_1"
        };
    case 1:
        return {
            tag: "_2"
        };
    case 2:
        return {
            tag: "_4"
        };
    case 3:
        return {
            tag: "_6"
        };
    case 4:
        return {
            tag: "_8"
        };
    case 5:
        return {
            tag: "_12"
        };
    case 6:
        return {
            tag: "_24"
        };
    case 7:
        return {
            tag: "_96"
        };
    case 8:
        return {
            tag: "_192"
        };
    case 9:
        return {
            tag: "_384"
        };
    default:
        throw "variant not implemented"
    }
}

function deserialize_CLOCK_SRC(d) {
    switch (d.deserialize_number(U32_BYTES, false)) {
    case 0:
        return {
            tag: "None"
        };
    case 1:
        return {
            tag: "Atom"
        };
    case 2:
        return {
            tag: "Meteor"
        };
    case 3:
        return {
            tag: "Cube"
        };
    case 4:
        return {
            tag: "Internal"
        };
    case 5:
        return {
            tag: "MidiIn"
        };
    case 6:
        return {
            tag: "MidiUsb"
        };
    default:
        throw "variant not implemented"
    }
}

function deserialize_COLOR(d) {
    switch (d.deserialize_number(U32_BYTES, false)) {
    case 0:
        return {
            tag: "White"
        };
    case 1:
        return {
            tag: "Yellow"
        };
    case 2:
        return {
            tag: "Orange"
        };
    case 3:
        return {
            tag: "Red"
        };
    case 4:
        return {
            tag: "Lime"
        };
    case 5:
        return {
            tag: "Green"
        };
    case 6:
        return {
            tag: "Cyan"
        };
    case 7:
        return {
            tag: "SkyBlue"
        };
    case 8:
        return {
            tag: "Blue"
        };
    case 9:
        return {
            tag: "Violet"
        };
    case 10:
        return {
            tag: "Pink"
        };
    case 11:
        return {
            tag: "PaleGreen"
        };
    case 12:
        return {
            tag: "Sand"
        };
    case 13:
        return {
            tag: "Rose"
        };
    case 14:
        return {
            tag: "Salmon"
        };
    case 15:
        return {
            tag: "LightBlue"
        };
    case 16:
        return {
            tag: "Custom",
            value: [
                d.deserialize_number(U8_BYTES, false),
                d.deserialize_number(U8_BYTES, false),
                d.deserialize_number(U8_BYTES, false)
            ]
        };
    default:
        throw "variant not implemented"
    }
}

function deserialize_CONFIG_MSG_IN(d) {
    switch (d.deserialize_number(U32_BYTES, false)) {
    case 0:
        return {
            tag: "Ping"
        };
    case 1:
        return {
            tag: "GetAllApps"
        };
    case 2:
        return {
            tag: "GetGlobalConfig"
        };
    case 3:
        return {
            tag: "SetGlobalConfig",
            value: deserialize_GLOBAL_CONFIG(d)
        };
    case 4:
        return {
            tag: "GetLayout"
        };
    case 5:
        return {
            tag: "SetLayout",
            value: deserialize_LAYOUT(d)
        };
    case 6:
        return {
            tag: "GetAllAppParams"
        };
    case 7:
        return {
            tag: "GetAppParams",
            value: {
                layout_id: d.deserialize_number(U8_BYTES, false)
            }
        };
    case 8:
        return {
            tag: "SetAppParams",
            value: {
                layout_id: d.deserialize_number(U8_BYTES, false),
                values: d.deserialize_array(() => (d.deserialize_number(U32_BYTES, false) === 0) ? undefined : deserialize_VALUE(d), 16)
            }
        };
    case 9:
        return {
            tag: "FactoryReset"
        };
    case 10:
        return {
            tag: "GetVersion"
        };
    default:
        throw "variant not implemented"
    }
}

function deserialize_CONFIG_MSG_OUT(d) {
    switch (d.deserialize_number(U32_BYTES, false)) {
    case 0:
        return {
            tag: "Pong"
        };
    case 1:
        return {
            tag: "BatchMsgStart",
            value: d.deserialize_number(U64_BYTES, false)
        };
    case 2:
        return {
            tag: "BatchMsgEnd"
        };
    case 3:
        return {
            tag: "GlobalConfig",
            value: deserialize_GLOBAL_CONFIG(d)
        };
    case 4:
        return {
            tag: "Layout",
            value: deserialize_LAYOUT(d)
        };
    case 5:
        return {
            tag: "AppConfig",
            value: [
                d.deserialize_number(U8_BYTES, false),
                d.deserialize_number(U64_BYTES, false),
                [
                    d.deserialize_number(U64_BYTES, false),
                    d.deserialize_string(),
                    d.deserialize_string(),
                    deserialize_COLOR(d),
                    deserialize_APP_ICON(d),
                    d.deserialize_array(() => deserialize_PARAM(d))
                ]
            ]
        };
    case 6:
        return {
            tag: "AppState",
            value: [
                d.deserialize_number(U8_BYTES, false),
                d.deserialize_array(() => deserialize_VALUE(d))
            ]
        };
    case 7:
        return {
            tag: "Version",
            value: {
                major: d.deserialize_number(U8_BYTES, false),
                minor: d.deserialize_number(U8_BYTES, false),
                patch: d.deserialize_number(U8_BYTES, false)
            }
        };
    default:
        throw "variant not implemented"
    }
}

function deserialize_CURVE(d) {
    switch (d.deserialize_number(U32_BYTES, false)) {
    case 0:
        return {
            tag: "Linear"
        };
    case 1:
        return {
            tag: "Logarithmic"
        };
    case 2:
        return {
            tag: "Exponential"
        };
    default:
        throw "variant not implemented"
    }
}

function deserialize_GLOBAL_CONFIG(d) {
    return {
        aux: d.deserialize_array(() => deserialize_AUX_JACK_MODE(d), 3),
        clock: deserialize_CLOCK_CONFIG(d),
        i2c_mode: deserialize_I_2_C_MODE(d),
        led_brightness: d.deserialize_number(U8_BYTES, false),
        midi: deserialize_MIDI_CONFIG(d),
        quantizer: deserialize_QUANTIZER_CONFIG(d),
        takeover_mode: deserialize_latch_TAKEOVER_MODE(d)
    };
}

function deserialize_I_2_C_MODE(d) {
    switch (d.deserialize_number(U32_BYTES, false)) {
    case 0:
        return {
            tag: "Calibration"
        };
    case 1:
        return {
            tag: "Leader"
        };
    case 2:
        return {
            tag: "Follower"
        };
    default:
        throw "variant not implemented"
    }
}

function deserialize_KEY(d) {
    switch (d.deserialize_number(U32_BYTES, false)) {
    case 0:
        return {
            tag: "Chromatic"
        };
    case 1:
        return {
            tag: "Ionian"
        };
    case 2:
        return {
            tag: "Dorian"
        };
    case 3:
        return {
            tag: "Phrygian"
        };
    case 4:
        return {
            tag: "Lydian"
        };
    case 5:
        return {
            tag: "Mixolydian"
        };
    case 6:
        return {
            tag: "Aeolian"
        };
    case 7:
        return {
            tag: "Locrian"
        };
    case 8:
        return {
            tag: "BluesMaj"
        };
    case 9:
        return {
            tag: "BluesMin"
        };
    case 10:
        return {
            tag: "PentatonicMaj"
        };
    case 11:
        return {
            tag: "PentatonicMin"
        };
    case 12:
        return {
            tag: "Folk"
        };
    case 13:
        return {
            tag: "Japanese"
        };
    case 14:
        return {
            tag: "Gamelan"
        };
    case 15:
        return {
            tag: "HungarianMin"
        };
    case 16:
        return {
            tag: "Off"
        };
    default:
        throw "variant not implemented"
    }
}

function deserialize_LAYOUT(d) {
    return [
        d.deserialize_array(() => (d.deserialize_number(U32_BYTES, false) === 0) ? undefined : [
            d.deserialize_number(U8_BYTES, false),
            d.deserialize_number(U64_BYTES, false),
            d.deserialize_number(U8_BYTES, false)
        ], 16)
    ];
}

function deserialize_MIDI_CC(d) {
    return [
        d.deserialize_number(U16_BYTES, false)
    ];
}

function deserialize_MIDI_CHANNEL(d) {
    return [
        d.deserialize_number(U8_BYTES, false)
    ];
}

function deserialize_MIDI_CONFIG(d) {
    return {
        outs: d.deserialize_array(() => deserialize_MIDI_OUT_CONFIG(d), 3)
    };
}

function deserialize_MIDI_IN(d) {
    return [
        d.deserialize_array(() => d.deserialize_bool(), 2)
    ];
}

function deserialize_MIDI_MODE(d) {
    switch (d.deserialize_number(U32_BYTES, false)) {
    case 0:
        return {
            tag: "Note"
        };
    case 1:
        return {
            tag: "Cc"
        };
    default:
        throw "variant not implemented"
    }
}

function deserialize_MIDI_NOTE(d) {
    return [
        d.deserialize_number(U8_BYTES, false)
    ];
}

function deserialize_MIDI_OUT(d) {
    return [
        d.deserialize_array(() => d.deserialize_bool(), 3)
    ];
}

function deserialize_MIDI_OUT_CONFIG(d) {
    return {
        send_clock: d.deserialize_bool(),
        send_transport: d.deserialize_bool(),
        mode: deserialize_MIDI_OUT_MODE(d)
    };
}

function deserialize_MIDI_OUT_MODE(d) {
    switch (d.deserialize_number(U32_BYTES, false)) {
    case 0:
        return {
            tag: "None"
        };
    case 1:
        return {
            tag: "Local"
        };
    case 2:
        return {
            tag: "MidiThru",
            value: {
                sources: deserialize_MIDI_IN(d)
            }
        };
    case 3:
        return {
            tag: "MidiMerge",
            value: {
                sources: deserialize_MIDI_IN(d)
            }
        };
    default:
        throw "variant not implemented"
    }
}

function deserialize_NOTE(d) {
    switch (d.deserialize_number(U32_BYTES, false)) {
    case 0:
        return {
            tag: "C"
        };
    case 1:
        return {
            tag: "CSharp"
        };
    case 2:
        return {
            tag: "D"
        };
    case 3:
        return {
            tag: "DSharp"
        };
    case 4:
        return {
            tag: "E"
        };
    case 5:
        return {
            tag: "F"
        };
    case 6:
        return {
            tag: "FSharp"
        };
    case 7:
        return {
            tag: "G"
        };
    case 8:
        return {
            tag: "GSharp"
        };
    case 9:
        return {
            tag: "A"
        };
    case 10:
        return {
            tag: "ASharp"
        };
    case 11:
        return {
            tag: "B"
        };
    default:
        throw "variant not implemented"
    }
}

function deserialize_PARAM(d) {
    switch (d.deserialize_number(U32_BYTES, false)) {
    case 0:
        return {
            tag: "None"
        };
    case 1:
        return {
            tag: "i32",
            value: {
                name: d.deserialize_string(),
                min: d.deserialize_number(U32_BYTES, true),
                max: d.deserialize_number(U32_BYTES, true)
            }
        };
    case 2:
        return {
            tag: "f32",
            value: {
                name: d.deserialize_string(),
                min: d.deserialize_number_float(U32_BYTES),
                max: d.deserialize_number_float(U32_BYTES)
            }
        };
    case 3:
        return {
            tag: "bool",
            value: {
                name: d.deserialize_string()
            }
        };
    case 4:
        return {
            tag: "Enum",
            value: {
                name: d.deserialize_string(),
                variants: d.deserialize_array(() => d.deserialize_string())
            }
        };
    case 5:
        return {
            tag: "Curve",
            value: {
                name: d.deserialize_string(),
                variants: d.deserialize_array(() => deserialize_CURVE(d))
            }
        };
    case 6:
        return {
            tag: "Waveform",
            value: {
                name: d.deserialize_string(),
                variants: d.deserialize_array(() => deserialize_WAVEFORM(d))
            }
        };
    case 7:
        return {
            tag: "Color",
            value: {
                name: d.deserialize_string(),
                variants: d.deserialize_array(() => deserialize_COLOR(d))
            }
        };
    case 8:
        return {
            tag: "Range",
            value: {
                name: d.deserialize_string(),
                variants: d.deserialize_array(() => deserialize_RANGE(d))
            }
        };
    case 9:
        return {
            tag: "Note",
            value: {
                name: d.deserialize_string(),
                variants: d.deserialize_array(() => deserialize_NOTE(d))
            }
        };
    case 10:
        return {
            tag: "MidiCc",
            value: {
                name: d.deserialize_string()
            }
        };
    case 11:
        return {
            tag: "MidiChannel",
            value: {
                name: d.deserialize_string()
            }
        };
    case 12:
        return {
            tag: "MidiIn"
        };
    case 13:
        return {
            tag: "MidiMode"
        };
    case 14:
        return {
            tag: "MidiNote",
            value: {
                name: d.deserialize_string()
            }
        };
    case 15:
        return {
            tag: "MidiOut"
        };
    case 16:
        return {
            tag: "MidiNrpn"
        };
    case 17:
        return {
            tag: "VoltPerOct"
        };
    default:
        throw "variant not implemented"
    }
}

function deserialize_QUANTIZER_CONFIG(d) {
    return {
        key: deserialize_KEY(d),
        tonic: deserialize_NOTE(d)
    };
}

function deserialize_RANGE(d) {
    switch (d.deserialize_number(U32_BYTES, false)) {
    case 0:
        return {
            tag: "_0_10V"
        };
    case 1:
        return {
            tag: "_0_5V"
        };
    case 2:
        return {
            tag: "_Neg5_5V"
        };
    default:
        throw "variant not implemented"
    }
}

function deserialize_RESET_SRC(d) {
    switch (d.deserialize_number(U32_BYTES, false)) {
    case 0:
        return {
            tag: "None"
        };
    case 1:
        return {
            tag: "Atom"
        };
    case 2:
        return {
            tag: "Meteor"
        };
    case 3:
        return {
            tag: "Cube"
        };
    default:
        throw "variant not implemented"
    }
}

function deserialize_latch_TAKEOVER_MODE(d) {
    switch (d.deserialize_number(U32_BYTES, false)) {
    case 0:
        return {
            tag: "Pickup"
        };
    case 1:
        return {
            tag: "Jump"
        };
    case 2:
        return {
            tag: "Scale"
        };
    default:
        throw "variant not implemented"
    }
}

function deserialize_VALUE(d) {
    switch (d.deserialize_number(U32_BYTES, false)) {
    case 0:
        return {
            tag: "i32",
            value: d.deserialize_number(U32_BYTES, true)
        };
    case 1:
        return {
            tag: "f32",
            value: d.deserialize_number_float(U32_BYTES)
        };
    case 2:
        return {
            tag: "bool",
            value: d.deserialize_bool()
        };
    case 3:
        return {
            tag: "Enum",
            value: d.deserialize_number(U64_BYTES, false)
        };
    case 4:
        return {
            tag: "Curve",
            value: deserialize_CURVE(d)
        };
    case 5:
        return {
            tag: "Waveform",
            value: deserialize_WAVEFORM(d)
        };
    case 6:
        return {
            tag: "Color",
            value: deserialize_COLOR(d)
        };
    case 7:
        return {
            tag: "Range",
            value: deserialize_RANGE(d)
        };
    case 8:
        return {
            tag: "Note",
            value: deserialize_NOTE(d)
        };
    case 9:
        return {
            tag: "MidiCc",
            value: deserialize_MIDI_CC(d)
        };
    case 10:
        return {
            tag: "MidiChannel",
            value: deserialize_MIDI_CHANNEL(d)
        };
    case 11:
        return {
            tag: "MidiIn",
            value: deserialize_MIDI_IN(d)
        };
    case 12:
        return {
            tag: "MidiMode",
            value: deserialize_MIDI_MODE(d)
        };
    case 13:
        return {
            tag: "MidiNote",
            value: deserialize_MIDI_NOTE(d)
        };
    case 14:
        return {
            tag: "MidiOut",
            value: deserialize_MIDI_OUT(d)
        };
    case 15:
        return {
            tag: "MidiNrpn",
            value: d.deserialize_bool()
        };
    case 16:
        return {
            tag: "VoltPerOct",
            value: deserialize_VOLT_PER_OCT(d)
        };
    default:
        throw "variant not implemented"
    }
}

function deserialize_VOLT_PER_OCT(d) {
    switch (d.deserialize_number(U32_BYTES, false)) {
    case 0:
        return {
            tag: "Standard"
        };
    case 1:
        return {
            tag: "Buchla"
        };
    default:
        throw "variant not implemented"
    }
}

function deserialize_WAVEFORM(d) {
    switch (d.deserialize_number(U32_BYTES, false)) {
    case 0:
        return {
            tag: "Triangle"
        };
    case 1:
        return {
            tag: "Saw"
        };
    case 2:
        return {
            tag: "SawInv"
        };
    case 3:
        return {
            tag: "Square"
        };
    case 4:
        return {
            tag: "Sine"
        };
    default:
        throw "variant not implemented"
    }
}

/**
 * Deserialize a value from an array of bytes.
 * @param {string} type - The type of the value to deserialize.
 * @param {Uint8Array} bytes - The byte array to deserialize from.
 * @return {Object} The deserialized value and remaining bytes.
 */
function deserialize(type, bytes) {
    if (!(typeof type === "string")) {
        throw "type must be a string";
    }
    const d = new Deserializer(bytes);
    var return_value = undefined;
    switch (type) {
    case "AppIcon":
        return_value = deserialize_APP_ICON(d);
        break;
    case "AuxJackMode":
        return_value = deserialize_AUX_JACK_MODE(d);
        break;
    case "ClockConfig":
        return_value = deserialize_CLOCK_CONFIG(d);
        break;
    case "ClockDivision":
        return_value = deserialize_CLOCK_DIVISION(d);
        break;
    case "ClockSrc":
        return_value = deserialize_CLOCK_SRC(d);
        break;
    case "Color":
        return_value = deserialize_COLOR(d);
        break;
    case "ConfigMsgIn":
        return_value = deserialize_CONFIG_MSG_IN(d);
        break;
    case "ConfigMsgOut":
        return_value = deserialize_CONFIG_MSG_OUT(d);
        break;
    case "Curve":
        return_value = deserialize_CURVE(d);
        break;
    case "GlobalConfig":
        return_value = deserialize_GLOBAL_CONFIG(d);
        break;
    case "I2cMode":
        return_value = deserialize_I_2_C_MODE(d);
        break;
    case "Key":
        return_value = deserialize_KEY(d);
        break;
    case "Layout":
        return_value = deserialize_LAYOUT(d);
        break;
    case "MidiCc":
        return_value = deserialize_MIDI_CC(d);
        break;
    case "MidiChannel":
        return_value = deserialize_MIDI_CHANNEL(d);
        break;
    case "MidiConfig":
        return_value = deserialize_MIDI_CONFIG(d);
        break;
    case "MidiIn":
        return_value = deserialize_MIDI_IN(d);
        break;
    case "MidiMode":
        return_value = deserialize_MIDI_MODE(d);
        break;
    case "MidiNote":
        return_value = deserialize_MIDI_NOTE(d);
        break;
    case "MidiOut":
        return_value = deserialize_MIDI_OUT(d);
        break;
    case "MidiOutConfig":
        return_value = deserialize_MIDI_OUT_CONFIG(d);
        break;
    case "MidiOutMode":
        return_value = deserialize_MIDI_OUT_MODE(d);
        break;
    case "Note":
        return_value = deserialize_NOTE(d);
        break;
    case "Param":
        return_value = deserialize_PARAM(d);
        break;
    case "QuantizerConfig":
        return_value = deserialize_QUANTIZER_CONFIG(d);
        break;
    case "Range":
        return_value = deserialize_RANGE(d);
        break;
    case "ResetSrc":
        return_value = deserialize_RESET_SRC(d);
        break;
    case "latch.TakeoverMode":
        return_value = deserialize_latch_TAKEOVER_MODE(d);
        break;
    case "Value":
        return_value = deserialize_VALUE(d);
        break;
    case "VoltPerOct":
        return_value = deserialize_VOLT_PER_OCT(d);
        break;
    case "Waveform":
        return_value = deserialize_WAVEFORM(d);
        break;
    default:
        throw "type not implemented";
    }
    return { value: return_value, bytes: d.release_bytes() };
}

export {
    deserialize
};
