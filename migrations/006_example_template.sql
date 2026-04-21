-- migrations/006_example_template.sql
--
-- Plantilla de ejemplo: experimento de retención de agua en macetas (Aníbal)
-- Solo se inserta si no existe ya una plantilla con este nombre.
-- Requiere que exista al menos un usuario admin (correr después del seed).

DO $$
DECLARE
    admin_id UUID;
    tmpl_id  UUID;
BEGIN
    -- Obtener el primer admin
    SELECT id INTO admin_id FROM users WHERE role = 'admin' LIMIT 1;
    IF admin_id IS NULL THEN
        RAISE NOTICE 'No hay admin todavía — corré el seed primero';
        RETURN;
    END IF;

    -- No insertar si ya existe
    IF EXISTS (SELECT 1 FROM experiment_templates WHERE name = 'Retención de agua en macetas') THEN
        RAISE NOTICE 'La plantilla ya existe';
        RETURN;
    END IF;

    INSERT INTO experiment_templates (owner_id, name, description, public, constants_schema, steps)
    VALUES (
        admin_id,
        'Retención de agua en macetas',
        'Caracterización de la capacidad volumétrica de retención de agua en suelos de maceta. Registro de pesadas periódicas e irrigaciones con cálculo automático de θ gravimétrico.',
        true,

        -- constants_schema: parámetros que el usuario llena UNA VEZ al iniciar el experimento
        '[
            {"key": "M_maceta",       "label": "Masa maceta vacía",             "unit": "g",     "comment_required": true},
            {"key": "M_tela",         "label": "Masa tela + cinta",             "unit": "g",     "comment_required": false},
            {"key": "M_sensor",       "label": "Masa sensor CS655",             "unit": "g",     "comment_required": false},
            {"key": "M_seco_aire",    "label": "Masa suelo seco al aire",       "unit": "g",     "comment_required": true,
             "comment_hint": "Pesaje antes de agregar agua. Base para cálculos de θ."},
            {"key": "theta_ini",      "label": "θ seco al aire (m³/m³)",        "unit": "m³/m³", "comment_required": false},
            {"key": "V_suelo",        "label": "Volumen de referencia",         "unit": "cm³",   "comment_required": true,
             "comment_hint": "Volumen del cono truncado calculado a partir de dimensiones."},
            {"key": "porosidad",      "label": "Porosidad",                     "unit": "adim",  "comment_required": false},
            {"key": "M_solidos",      "label": "Masa de sólidos",               "unit": "g",     "comment_required": false},
            {"key": "theta_objetivo", "label": "θ objetivo para el experimento","unit": "m³/m³", "comment_required": true,
             "comment_hint": "Ej: 0.30 para 30% de humedad volumétrica."},
            {"key": "tolerancia",     "label": "Tolerancia de θ (±)",           "unit": "m³/m³", "comment_required": false,
             "default": 0.02}
        ]',

        -- steps: grafo FSM con scripts Rhai
        '[
            {
                "key":   "medir_dimensiones",
                "label": "Medición de dimensiones de la maceta",
                "type":  "measurement_group",
                "fields": [
                    {"key": "diametro_inferior", "label": "Diámetro inferior", "unit": "cm"},
                    {"key": "diametro_superior", "label": "Diámetro superior", "unit": "cm"},
                    {"key": "altura_suelo",      "label": "Altura del suelo",  "unit": "cm"}
                ],
                "script": "
                    let r1 = steps.diametro_inferior.value / 2.0;
                    let r2 = steps.diametro_superior.value / 2.0;
                    let h  = steps.altura_suelo.value;
                    let V  = (3.14159 / 3.0) * h * (r1*r1 + r1*r2 + r2*r2);
                    output(\"V_cono\", V, \"cm³\");
                    log(\"Volumen del cono truncado: \" + V + \" cm³\");
                    log(\"Guardá este valor como V_suelo en las constantes.\");
                    goto(\"pesar_seco\");
                ",
                "next": [{"condition": "true", "goto": "pesar_seco"}]
            },
            {
                "key":   "pesar_seco",
                "label": "Pesaje inicial — suelo seco al aire",
                "type":  "measurement",
                "unit":  "g",
                "hint":  "Pesá la maceta completa (con tela, suelo seco, sin sensor ni agua)",
                "script": "
                    let M_actual = steps.pesar_seco.value;
                    let M_suelo  = M_actual - constants.M_maceta - constants.M_tela;
                    output(\"M_suelo_seco\", M_suelo, \"g\");
                    log(\"Masa suelo seco: \" + M_suelo + \" g\");
                    log(\"Usá este valor como M_seco_aire en las constantes.\");
                    goto(\"calcular_agua_inicial\");
                ",
                "next": [{"condition": "true", "goto": "calcular_agua_inicial"}]
            },
            {
                "key":   "calcular_agua_inicial",
                "label": "Calcular agua necesaria para θ objetivo",
                "type":  "calculation",
                "script": "
                    let theta_ini = constants.theta_ini;
                    let theta_obj = constants.theta_objetivo;
                    let V = constants.V_suelo;
                    let agua = (theta_obj - theta_ini) * V;
                    output(\"agua_a_añadir\", agua, \"g\");
                    log(\"θ inicial: \" + theta_ini);
                    log(\"θ objetivo: \" + theta_obj);
                    log(\"Agua a añadir: \" + agua + \" g\");
                    plot(\"theta_progress\", #{
                        title: \"Evolución de θ\",
                        series: \"peso_continuo\",
                        x: \"recorded_at\",
                        y_formula: \"(value - constants.M_solidos) / constants.V_suelo\",
                        y_label: \"θ gravimétrico (m³/m³)\",
                        type: \"line\",
                        reference_line: constants.theta_objetivo
                    });
                    goto(\"irrigar\");
                ",
                "next": [{"condition": "true", "goto": "irrigar"}]
            },
            {
                "key":   "irrigar",
                "label": "Irrigación",
                "type":  "measurement_group",
                "hint":  "Pesá el agua antes y después de aplicar",
                "fields": [
                    {"key": "agua_calculada", "label": "Agua calculada", "unit": "g"},
                    {"key": "agua_real",      "label": "Agua real aplicada", "unit": "g"},
                    {"key": "duracion_s",     "label": "Duración aplicación", "unit": "s"}
                ],
                "script": "
                    let agua_calc = steps.irrigar.agua_calculada;
                    let agua_real = steps.irrigar.agua_real;
                    let diff = agua_real - agua_calc;
                    output(\"diferencia_agua\", diff, \"g\");
                    if diff > 10.0 {
                        warn(\"Diferencia de agua > 10g — verificar drenaje\");
                    }
                    log(\"Irrigación registrada: \" + agua_real + \" g aplicados\");
                    goto(\"pesar_post_irrigacion\");
                ",
                "next": [{"condition": "true", "goto": "pesar_post_irrigacion"}]
            },
            {
                "key":   "pesar_post_irrigacion",
                "label": "Pesaje post-irrigación",
                "type":  "measurement",
                "unit":  "g",
                "hint":  "Pesá con sensor instalado. Restará M_sensor automáticamente.",
                "script": "
                    let M_total  = steps.pesar_post_irrigacion.value;
                    let M_suelo_agua = M_total - constants.M_maceta - constants.M_tela - constants.M_sensor;
                    let theta = (M_suelo_agua - constants.M_solidos) / constants.V_suelo;
                    let saturacion = theta / constants.porosidad;
                    output(\"M_suelo_agua\",  M_suelo_agua, \"g\");
                    output(\"theta_actual\",  theta,        \"m³/m³\");
                    output(\"saturacion\",    saturacion * 100.0, \"%\");
                    log(\"θ actual: \" + theta + \" m³/m³\");
                    log(\"Saturación: \" + (saturacion * 100.0) + \"%\");
                    table(\"historial_irrigaciones\", #{
                        series: \"irrigar\",
                        columns: [\"recorded_at\", \"data.agua_real\", \"data.duracion_s\"]
                    });
                    goto(\"monitoreo_continuo\");
                ",
                "next": [{"condition": "true", "goto": "monitoreo_continuo"}]
            },
            {
                "key":   "monitoreo_continuo",
                "label": "Pesaje de monitoreo",
                "type":  "repeat",
                "unit":  "g",
                "hint":  "Pesá la maceta varias veces al día (sin sensor)",
                "script": "
                    let M_total = steps.monitoreo_continuo.value;
                    let M_suelo_agua = M_total - constants.M_maceta - constants.M_tela;
                    let theta = (M_suelo_agua - constants.M_solidos) / constants.V_suelo;
                    let tol = constants.tolerancia;
                    let theta_obj = constants.theta_objetivo;
                    output(\"theta_actual\", theta, \"m³/m³\");
                    log(\"θ actual: \" + theta + \" m³/m³  |  objetivo: \" + theta_obj);
                    plot(\"perdida_agua\", #{
                        title: \"Pérdida de agua en el tiempo\",
                        series: \"monitoreo_continuo\",
                        x: \"recorded_at\",
                        y: \"value\",
                        y_label: \"Masa (g)\",
                        type: \"line\"
                    });
                    if theta < theta_obj - tol {
                        warn(\"θ por debajo del objetivo. Necesita irrigación.\");
                        goto(\"calcular_agua_inicial\");
                    } else {
                        log(\"Suelo dentro del rango objetivo. Continuar monitoreo.\");
                        goto(\"monitoreo_continuo\");
                    }
                ",
                "next": [
                    {
                        "condition": "steps.theta_actual.value < constants.theta_objetivo - constants.tolerancia",
                        "goto": "calcular_agua_inicial",
                        "label": "Necesita irrigación"
                    },
                    {
                        "condition": "true",
                        "goto": "monitoreo_continuo",
                        "label": "Continuar monitoreo"
                    }
                ]
            }
        ]'
    )
    RETURNING id INTO tmpl_id;

    RAISE NOTICE 'Plantilla creada con id: %', tmpl_id;
END $$;
