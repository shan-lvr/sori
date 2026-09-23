import CoreAudio
var addr = AudioObjectPropertyAddress(mSelector: kAudioHardwarePropertyDefaultInputDevice, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
var dev = AudioDeviceID(0); var size = UInt32(MemoryLayout<AudioDeviceID>.size)
AudioObjectGetPropertyData(AudioObjectID(kAudioObjectSystemObject), &addr, 0, nil, &size, &dev)
var running: UInt32 = 0; size = 4
addr.mSelector = kAudioDevicePropertyDeviceIsRunningSomewhere
AudioObjectGetPropertyData(dev, &addr, 0, nil, &size, &running)
print("default input device running somewhere:", running != 0)
