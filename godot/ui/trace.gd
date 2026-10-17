# SPDX-License-Identifier: GPL-3.0-or-later
extends RefCounted
## UI fixtures can run without the native extension.

static func begin(label: String) -> RefCounted:
	if not ClassDB.class_exists("UiTraceSpan"):
		return null
	var span: RefCounted = ClassDB.instantiate("UiTraceSpan")
	span.call("start", label)
	return span
