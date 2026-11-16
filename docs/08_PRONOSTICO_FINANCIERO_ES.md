# Pronóstico financiero — NeXXUs

| Campo | Valor |
|-------|--------|
| **Producto** | NeXXUs — nodo soberano de almacenamiento y plataforma de aplicaciones |
| **Versión de referencia** | 2.0.0-beta.1 (núcleo técnico auditado) |
| **Horizonte** | 3 años tras el lanzamiento público |
| **Moneda** | USD |
| **Fecha del informe** | 16 de noviembre de 2026 |
| **Tipo** | Pronóstico de escenarios (no es un presupuesto auditado) |

---

## 1. Resumen ejecutivo

NeXXUs no se modela como un «disco en la nube barato» que cobra por GB. La arquitectura de negocio es:

- **Caja fuerte personal gratuita** (mismo clave BIP39 / identidad del nodo).
- **Nodo doméstico** en teléfono antiguo **solo** con alimentación 24/7 y Wi‑Fi.
- **Cada nodo = servicio onion** (privacidad de acceso; la velocidad no es el KPI).
- **Supervivencia de datos**: si el nodo muere, otro dispositivo + importación de claves + restauración lenta (días) desde la red.
- **Ingresos** desde atención y servicios: fan/suscripciones, reels, publicidad opt-in, premium, tienda de apps, nodos Linux de alta capacidad.
- **No se cobra** el archivo personal / favoritos del mensajero.

**Escenario base (realista) a 3 años:** del orden de **18–40 MUSD de ingresos anuales**, **0,6–1,2 M** de nodos always-on y **1,2–2,5 M** de MAU de ecosistema, con margen alto en comisiones de software y casi **cero COGS** de almacenamiento en teléfonos ajenos.

---

## 2. Premisas de producto (base del modelo financiero)

| Elemento | Implicación financiera |
|----------|------------------------|
| Teléfono 24/7 + Wi‑Fi | Nodo tipo *appliance*; electricidad y hardware del usuario |
| Una clave = nodo + apps | Mayor retención; menor fricción entre mensajero, vault y juegos |
| Onion (+ bridge si hace falta) | Se vende privacidad, no latencia de CDN |
| Restauración en días | Sin coste de SLA tipo AWS |
| Mensajero para paranoicos | Uso diario; favoritos fuera de Big Tech |
| Reels / fan / ads | Motor principal de caja |
| Tienda + SDK tipo LEGO | Apps de terceros → GMV y comisión |
| Linux + enlace gigabit | Pin, IA local, ancho de banda como servicio de pago |
| Descargo de responsabilidad | La velocidad no es el objetivo; sí lo son privacidad, anonimato de acceso y durabilidad |

Prueba interna de referencia: en red doméstica el nodo es visible por onion y el rendimiento es **aceptable** para las funciones declaradas (vault, mensajería, metadatos, medios ligeros).

---

## 3. Flujos de ingreso

| ID | Flujo | Quién paga | Take-rate orientativo |
|----|--------|------------|------------------------|
| A | Fan / suscripciones / propinas | Espectador → creador | **15%** plataforma |
| B | Reels / promoción | Creador o anunciante | CPM o boost |
| C | Publicidad opt-in | Anunciante (solo *discovery*, no el vault) | eCPM inferior a apps masivas |
| D | Premium | Usuario | **2–5 USD/mes** |
| E | Tienda de aplicaciones | Apps de pago / IAP | **15–20%** |
| F | Servicios de nodos Linux | Apps, creadores, equipos | **10–15%** sobre pin/IA (o SaaS del operador) |
| G | B2B white-label | ONG, redacciones, comunidades | Contratos 5k–50k+ USD/año |

**Fuera del modelo:** tarifa por GB del propio archivo del usuario; token obligatorio para abrir el chat.

---

## 4. Economía unitaria (escenario realista)

| Indicador | Orden de magnitud |
|-----------|-------------------|
| ARPU de usuario de pago (fan + premium + ads atribuidos) | **2–6 USD / mes** |
| % de MAU que paga | **3–8%** (menor en año 1, mayor en año 3) |
| CAC (orgánico + nicho privacy) | **1–5 USD** en realismo |
| Coste de un nodo-teléfono para la plataforma | **~0 USD** |
| Gastos de infra propios | Relay/intro, tienda, CI, moderación, soporte (crecen con MAU) |
| Nodo Linux | La plataforma no paga su disco; cobra **comisión** por servicios vendidos |

La margen bruta debe apoyarse en **comisiones y software**, no en revender almacenamiento personal.

---

## 5. Escenarios a 3 años (ingresos de la plataforma)

Los importes son **ingresos netos de la plataforma** (take-rate), no el GMV total pagado a creadores.

### 5.1 Pesimismo (~25–30 % de probabilidad)

| | Año 1 | Año 2 | Año 3 |
|--|-------|-------|-------|
| Nodos always-on | 10–25 k | 30–60 k | 50–100 k |
| MAU | 20–50 k | 60–120 k | 100–200 k |
| **Ingresos** | **0,15–0,5 MUSD** | **0,5–1,5 MUSD** | **1–3 MUSD** |
| EBITDA | Negativo | ≈0 / leve negativo | Leve positivo / 0 |

Lectura: herramienta de nicho privacy; fan/reels y tienda no despegan.

### 5.2 Realismo (~45–50 %) — plan base

| | Año 1 | Año 2 | Año 3 |
|--|-------|-------|-------|
| Nodos always-on | 50–100 k | 250–500 k | **0,6–1,2 M** |
| MAU ecosistema | 100–200 k | 0,4–0,8 M | **1,2–2,5 M** |
| Usuarios de pago (media mensual) | 4–12 k | 25–60 k | **60–150 k** |
| Proveedores Linux | 50–200 | 500–2 k | **3–8 k** |
| Apps en tienda | 3–8 (propias) | 20–60 | **80–200** |
| **Ingresos** | **0,8–2,5 MUSD** | **6–15 MUSD** | **18–40 MUSD** |

**Ingresos acumulados en 3 años (realismo):** ~**25–55 MUSD**.  
Hacia el final del año 3 el negocio puede ser **operativamente sostenible** sin tarificar el vault personal.

### 5.3 Optimismo (~20–25 %)

| | Año 1 | Año 2 | Año 3 |
|--|-------|-------|-------|
| Nodos always-on | 200–400 k | 1–2 M | **3–7 M** |
| MAU | 0,5–1 M | 2–5 M | **8–15 M** |
| **Ingresos** | **3–8 MUSD** | **25–60 MUSD** | **80–180 MUSD** |

Motor: viralidad del mensajero y/o fan+reels, más apps de terceros sobre el SDK.

---

## 6. Mezcla de ingresos (realismo, año 3)

| Fuente | Peso orientativo |
|--------|------------------|
| Fan / suscripciones / propinas | 40–50 % |
| Premium | 15–20 % |
| Publicidad (solo discovery) | 10–15 % |
| Tienda de apps | 10–15 % |
| Comisión nodos Linux (pin, IA, ancho de banda) | 10–15 % |
| B2B white-label | 5–10 % |

El GMV de creadores puede ser **5–8×** los ingresos de la plataforma.

---

## 7. Gastos operativos (realismo, orden de magnitud)

| Partida | Año 1 | Año 2 | Año 3 |
|---------|-------|-------|-------|
| I+D (core, SDK, apps) | 1,5–3 M | 3–6 M | 5–10 M |
| Relay / intro / plano de control | 0,2–0,6 M | 0,8–2 M | 2–5 M |
| Moderación y trust & safety | 0,2–0,5 M | 1–3 M | 3–8 M |
| Tienda, pagos, soporte | 0,2–0,5 M | 1–2 M | 2–4 M |
| Marketing (prudente) | 0,3–1 M | 1–3 M | 2–6 M |
| **Opex total** | **~2,5–5,5 M** | **~7–16 M** | **~15–35 M** |

Años 1–2 suelen ser de **inversión** (ingresos < opex).  
Break-even orientativo: **final del año 2 / año 3**, si fan + tienda funcionan.

---

## 8. Necesidad de capital hasta autofinanciación

| Escenario | Capital orientativo |
|-----------|---------------------|
| Pesimismo | 3–6 MUSD (riesgo de quedarse en nicho) |
| **Realismo** | **8–15 MUSD** (producto + SDK + tienda + safety + 18–30 meses de runway) |
| Optimismo | 10–20 MUSD para crecimiento (retorno antes) |

Nota: el coste de construir el **núcleo técnico** hasta beta (~0,35–0,55 MUSD en equipo humano *from scratch*) es solo la base de ingeniería. El lanzamiento público con tienda, SDK, moderación fan y mensajero es **otra etapa**.

---

## 9. Hitos por año (lectura financiera)

### Año 1 — «Demostrar el rito»
- Onboarding: cargador + Wi‑Fi + clave + onion.
- Mensajero con favoritos en vault propio.
- UX de restauración (progreso en días).
- Tienda con 2–3 apps propias; SDK usable internamente.
- **Ingresos realistas:** 0,8–2,5 MUSD; foco en product-market fit.

### Año 2 — «El vault es hábito; el feed es caja»
- Reels/fan activos; comisión estable.
- Apps de terceros en la tienda.
- Nodos Linux venden pin/IA.
- **Ingresos realistas:** 6–15 MUSD.

### Año 3 — «Capa, no solo una app»
- NeXXUs como backend de muchos clientes.
- 0,6–1,2 M nodos always-on (realismo).
- **Ingresos realistas:** 18–40 MUSD; vía a EBITDA positivo.

---

## 10. Análisis de sensibilidad

| Si ocurre… | Efecto probable |
|------------|-----------------|
| UX pobre de BIP39 / restore | MAU y nodos × 0,3–0,5 |
| Publicidad dentro del vault cifrado | Fuga de usuarios «paranoicos» |
| Fan/reels no despegan | Techo ~3–8 MUSD en año 3 (premium + B2B) |
| SDK difícil | Tienda vacía; solo apps propias |
| Nodos Linux sin demanda de servicios | Sin flujo F |
| Regulación dura (18+ / crypto) | Golpe a A/B; hace falta plan B (utilidades, B2B) |

---

## 11. Comparación con Big Tech

No se proyecta sustituir S3 ni Google Drive enteros.  
Se proyecta **cuota en**:

- vault personal cifrado / favoritos del mensajero;
- medios de creadores con backend no monopolizado;
- edge de nodos domésticos onion.

El valor financiero está en **ser la capa soberana + el mercado de atención**, no en ganar la guerra del milisegundo.

---

## 12. Conclusión

| Pregunta | Respuesta |
|----------|-----------|
| ¿Negocio grande con vault personal a **0 USD/GB**? | **Sí**, si la caja viene de fan, tienda, premium y servicios de nodo |
| Base año 3 (realismo) | **~18–40 MUSD** de ingresos / año |
| Acumulado 3 años (realismo) | **~25–55 MUSD** |
| Motor de dinero | Mensajero → atención → fan/reels/tienda |
| Rol de nodos Linux | Caja secundaria de alto margen |
| Pesimismo año 3 | **1–3 MUSD**/año (nicho) |
| Optimismo año 3 | **80–180 MUSD**/año |

**En una frase:** con la arquitectura actual, el pronóstico realista a tres años tras el lanzamiento es una compañía de **decenas de millones de dólares de ingresos anuales**, alta margen bruta sobre comisiones, COGS mínimo en almacenamiento doméstico ajeno, y disciplina de producto: **privacidad y durabilidad primero; la velocidad no se vende como promesa central; el archivo personal no se tarifica.**

---

## 13. Aviso

Este documento es un **pronóstico de escenarios** para planificación e inversores. No constituye garantía de resultados, valoración formal ni consejo de inversión. Las cifras dependen de ejecución de producto, regulación, moderación y adopción real del rito «segundo teléfono en el enchufe + Wi‑Fi».

---

*NeXXUs — informe de pronóstico financiero (ES) — 16/11/2026*
