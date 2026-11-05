// `dictate-ane system-audio`: what the Mac plays (every app, e.g. the other people in a call), for
// the Souffleur (`src/meeting.rs`). A Core Audio process tap on all output (macOS 14.2+) inside a
// private aggregate device; macOS asks once for "System Audio Recording" (the app's
// NSAudioCaptureUsageDescription). Without that permission the tap delivers silence.
//
//   stdout: "rate <hz>\n", then mono little-endian f32 at that rate, until stdin closes (or the
//   process is killed). Errors: "error <message>\n" instead of the rate line, exit 1.

import AudioToolbox
import CoreAudio
import Foundation

private func check(_ status: OSStatus, _ what: String) {
    if status != noErr {
        reply("error \(what): OSStatus \(status)")
        exit(1)
    }
}

private func address(_ selector: AudioObjectPropertySelector) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress(
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain)
}

/// The UID of the device the Mac plays through now; the aggregate device clocks from it.
private func outputUID() -> String {
    var device = AudioObjectID(kAudioObjectUnknown)
    var size = UInt32(MemoryLayout<AudioObjectID>.size)
    var where_ = address(kAudioHardwarePropertyDefaultSystemOutputDevice)
    check(
        AudioObjectGetPropertyData(
            AudioObjectID(kAudioObjectSystemObject), &where_, 0, nil, &size, &device),
        "default output device")
    var uid: Unmanaged<CFString>?
    size = UInt32(MemoryLayout<Unmanaged<CFString>?>.size)
    where_ = address(kAudioDevicePropertyDeviceUID)
    check(AudioObjectGetPropertyData(device, &where_, 0, nil, &size, &uid), "output device UID")
    return uid?.takeRetainedValue() as String? ?? ""
}

func systemAudio() {
    guard #available(macOS 14.2, *) else {
        reply("error Systemton braucht macOS 14.2 oder neuer")
        exit(1)
    }
    let description = CATapDescription(stereoGlobalTapButExcludeProcesses: [])
    description.uuid = UUID()
    description.name = "Dictate Souffleur"
    description.isPrivate = true
    description.muteBehavior = .unmuted
    var tap = AudioObjectID(kAudioObjectUnknown)
    check(AudioHardwareCreateProcessTap(description, &tap), "create the tap")

    var format = AudioStreamBasicDescription()
    var size = UInt32(MemoryLayout<AudioStreamBasicDescription>.size)
    var where_ = address(kAudioTapPropertyFormat)
    check(AudioObjectGetPropertyData(tap, &where_, 0, nil, &size, &format), "tap format")
    guard format.mFormatID == kAudioFormatLinearPCM,
        format.mFormatFlags & kAudioFormatFlagIsFloat != 0, format.mBitsPerChannel == 32
    else {
        reply("error tap format is not 32-bit float (flags \(format.mFormatFlags))")
        exit(1)
    }

    let output = outputUID()
    let aggregate: [String: Any] = [
        kAudioAggregateDeviceNameKey: "Dictate Souffleur",
        kAudioAggregateDeviceUIDKey: UUID().uuidString,
        kAudioAggregateDeviceMainSubDeviceKey: output,
        kAudioAggregateDeviceIsPrivateKey: true,
        kAudioAggregateDeviceIsStackedKey: false,
        kAudioAggregateDeviceTapAutoStartKey: true,
        kAudioAggregateDeviceSubDeviceListKey: [[kAudioSubDeviceUIDKey: output]],
        kAudioAggregateDeviceTapListKey: [
            [
                kAudioSubTapDriftCompensationKey: true,
                kAudioSubTapUIDKey: description.uuid.uuidString,
            ]
        ],
    ]
    var device = AudioObjectID(kAudioObjectUnknown)
    check(
        AudioHardwareCreateAggregateDevice(aggregate as CFDictionary, &device),
        "create the aggregate device")
    // The IO proc delivers at the aggregate device's rate, which is the output device's, not the
    // tap's (48 kHz): a Bluetooth headset in headset mode (its microphone on, as in a call) plays at
    // 16 kHz, and taken for 48 kHz the others came out three times too fast.
    var rate = format.mSampleRate
    size = UInt32(MemoryLayout<Float64>.size)
    where_ = address(kAudioDevicePropertyNominalSampleRate)
    check(AudioObjectGetPropertyData(device, &where_, 0, nil, &size, &rate), "device rate")

    let interleaved = format.mFormatFlags & kAudioFormatFlagIsNonInterleaved == 0
    let writer = DispatchQueue(label: "system-audio-out")
    var proc: AudioDeviceIOProcID?
    check(
        AudioDeviceCreateIOProcIDWithBlock(&proc, device, nil) { _, input, _, _, _ in
            let buffers = UnsafeMutableAudioBufferListPointer(UnsafeMutablePointer(mutating: input))
            guard let first = buffers.first, first.mDataByteSize > 0 else { return }
            let channels = interleaved ? Int(max(first.mNumberChannels, 1)) : buffers.count
            let frames = Int(first.mDataByteSize) / 4 / (interleaved ? channels : 1)
            var mono = [Float](repeating: 0, count: frames)
            for (index, buffer) in buffers.enumerated() {
                guard let data = buffer.mData?.assumingMemoryBound(to: Float.self) else { continue }
                if interleaved {
                    guard index == 0 else { break }
                    for frame in 0..<frames {
                        var sum: Float = 0
                        for channel in 0..<channels { sum += data[frame * channels + channel] }
                        mono[frame] = sum / Float(channels)
                    }
                } else {
                    for frame in 0..<frames { mono[frame] += data[frame] / Float(channels) }
                }
            }
            let bytes = mono.withUnsafeBufferPointer { Data(buffer: $0) }
            // Not from the real-time thread: a slow reader must not stall the audio.
            writer.async { FileHandle.standardOutput.write(bytes) }
        }, "create the IO proc")

    reply("rate \(Int(rate))")
    check(AudioDeviceStart(device, proc), "start")
    log("system audio: tapping \(output) at \(Int(rate)) Hz (tap \(Int(format.mSampleRate)) Hz)")
    // Runs until dictate closes our stdin (or kills us).
    while !FileHandle.standardInput.availableData.isEmpty {}
    AudioDeviceStop(device, proc)
    if let proc { AudioDeviceDestroyIOProcID(device, proc) }
    AudioHardwareDestroyAggregateDevice(device)
    AudioHardwareDestroyProcessTap(tap)
}
