// Copyright 2026 openduo and OpenDuo contributors
// SPDX-License-Identifier: FSL-1.1-Apache-2.0
// The untested Sight layout helper needs this type. No layout/camera behavior is vectorized.
// Avoid importing UI/thread/Markdown dependencies into a standalone audio protocol generator.
public enum DuoLayout {
    case phone, tent, side
    case stacked(top: Double, gap: Double)
    case tabletop(top: Double, gap: Double)
}
