package br.com.oeratech.ingress;

import br.com.oeratech.ingress.dto.RouteDTO;
import br.com.oeratech.ingress.dto.RouteTarget;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.networknt.schema.JsonSchemaFactory;
import com.networknt.schema.SpecVersion;
import jakarta.annotation.PostConstruct;
import jakarta.enterprise.context.ApplicationScoped;
import org.jboss.logging.Logger;

import java.io.InputStream;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.Set;
import java.util.stream.Collectors;

/**
 * Carrega o mapa de rotas (routes.json) uma única vez na inicialização.
 * Em imagem nativa isso acontece durante o cold start, em milissegundos.
 */
@ApplicationScoped
public class RouteConfigService {

    private static final Logger LOG = Logger.getLogger(RouteConfigService.class);

    private final Map<String, RouteTarget> routes = new HashMap<>();

    @PostConstruct
    void init() {
        try (InputStream is = getClass().getClassLoader().getResourceAsStream("routes.json")) {
            if (is == null) {
                throw new IllegalStateException("routes.json not found on classpath");
            }

            ObjectMapper mapper = new ObjectMapper();
            List<RouteDTO> dtos = mapper.readValue(is,
                    mapper.getTypeFactory().constructCollectionType(List.class, RouteDTO.class));

            JsonSchemaFactory factory = JsonSchemaFactory.getInstance(SpecVersion.VersionFlag.V7);

            for (RouteDTO dto : dtos) {
                RouteTarget target = new RouteTarget(
                        dto.queueName(),
                        dto.schema() != null ? factory.getSchema(dto.schema()) : null);

                routes.put(dto.headerValue(), target);
                LOG.infof("route registered: %s -> %s (schema=%b)",
                        dto.headerValue(), dto.queueName(), dto.schema() != null);
            }

            LOG.infof("routes loaded: %d total", routes.size());
        } catch (Exception e) {
            throw new IllegalStateException("Failed to load routes.json", e);
        }
    }

    public Optional<RouteTarget> findRoute(String headerValue) {
        return Optional.ofNullable(routes.get(headerValue));
    }

    public Set<String> queueNames() {
        return routes.values().stream()
                .map(RouteTarget::queueName)
                .collect(Collectors.toSet());
    }
}
