package after_resolution

import rego.v1

namespace := "t3code."

deny contains finding if {
	some attr in input.registry.attributes
	not startswith(attr.key, namespace)

	finding := {
		"id": "t3code_attribute_namespace",
		"context": {"attribute_key": attr.key},
		"message": sprintf("Attribute '%s' must be namespaced under '%s'.", [attr.key, namespace]),
		"level": "violation",
	}
}

deny contains finding if {
	some metric in input.registry.metrics
	not startswith(metric.name, namespace)

	finding := {
		"id": "t3code_metric_namespace",
		"context": {"metric_name": metric.name},
		"message": sprintf("Metric '%s' must be namespaced under '%s'.", [metric.name, namespace]),
		"level": "violation",
	}
}
