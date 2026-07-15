package br.com.oeratech.ingress.dto;

import com.networknt.schema.JsonSchema;
import io.quarkus.runtime.annotations.RegisterForReflection;

@RegisterForReflection
public record RouteTarget(String queueName, JsonSchema schema) {
}
