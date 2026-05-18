(function () {
    const viewerVersion = window.SCRATCHPAD_VIEWER_VERSION || "dev";
    const sources = {
        catalog: `../target/analysis/measurement_catalog.json?v=${viewerVersion}`,
        runs: `../target/analysis/measurement_runs.json?v=${viewerVersion}`,
        hotspots: `../target/analysis/hotspots.json?v=${viewerVersion}`,
        slowspots: `../target/analysis/slowspots.json?v=${viewerVersion}`,
        searchSpeed: `../target/analysis/search_speed.json?v=${viewerVersion}`,
        capacityReport: `../target/analysis/capacity_report.json?v=${viewerVersion}`,
        resourceProfiles: `../target/analysis/resource_profiles.json?v=${viewerVersion}`,
        speedReport: `../target/analysis/speed_efficiency_report.json?v=${viewerVersion}`,
        performanceReview: `../target/analysis/performance_review.json?v=${viewerVersion}`,
        clones: `../target/analysis/clones.json?v=${viewerVersion}`,
        typeHealth: `../target/analysis/type_health.json?v=${viewerVersion}`,
        escapeHatches: `../target/analysis/rust_escape_hatches.json?v=${viewerVersion}`,
        locality: `../target/analysis/locality_metrics.json?v=${viewerVersion}`,
        leverage: `../target/analysis/leverage_metrics.json?v=${viewerVersion}`,
        map: `../target/analysis/map.json?v=${viewerVersion}`,
        projectCodeMetrics: `../target/analysis/project_code_metrics.json?v=${viewerVersion}`,
        flamegraphs: `../target/analysis/flamegraphs.json?v=${viewerVersion}`,
        correctness: `../target/analysis/correctness_review.json?v=${viewerVersion}`,
        appPackage: `/api/app-package?v=${viewerVersion}`,
    };

    const state = {
        catalog: null,
        runs: [],
        hotspots: [],
        slowspots: [],
        searchSpeed: [],
        capacityReport: null,
        resourceProfiles: null,
        speedReport: null,
        performanceReview: null,
        clones: [],
        typeHealth: [],
        escapeHatches: [],
        locality: [],
        leverage: [],
        map: null,
        projectCodeMetrics: null,
        flamegraphs: [],
        correctness: null,
        appPackage: null,
        selectedModule: null,
        selectedFlamegraph: null,
        selectedRun: null,
        selectedLayer: null,
        selectedCorrectnessCategory: null,
        selectedPerformanceScenarioId: null,
        selectedFlamegraphsByScenario: {},
        moduleRollupSort: { key: "combined", direction: "desc" },
        lastObservedFinishedRun: null,
        mapZoom: 0.65,
        mapLayout: 'folder',
        mapMetric: 'total_score',
        focusMode: false,
        overviewRiskMode: 'top',
        overviewRiskFilter: 'all',
        qualityDistributionMode: 'counts',
        cloneDistributionMode: 'counts',
        typeHealthDistributionMode: 'counts',
        escapeHatchDistributionMode: 'counts',
        localityDistributionMode: 'counts',
        leverageDistributionMode: 'counts',
        typeHealthScatterY: 'methods',
        expandedQualityKey: null,
        expandedCloneKey: null,
        expandedTypeHealthKey: null,
        expandedEscapeHatchKey: null,
        expandedLocalityKey: null,
        expandedLeverageKey: null,
        qualityDatasetView: 'hotspots',
        performanceDatasetSearch: '',
        performanceBucketFilters: {},
        performanceDistributionModes: {
            budget: 'counts',
            scaling: 'counts',
            capacity: 'counts',
            resources: 'counts',
        },
        searchChartScope: 'tabs',
        appPackageView: 'diagnostics',
    };

    const activeRunStatuses = new Set(["queued", "running"]);

    const formatNumber = new Intl.NumberFormat(undefined, {
        maximumFractionDigits: 2,
    });

    const searchModeColors = {
        active: "#6fd0ff",
        current: "#f3c969",
        all: "#c7a6ff",
    };

    const searchLatencyColors = {
        completion: "#6fd0ff",
        first_response: "#7ddc9b",
    };

    const searchLatencyDash = {
        completion: "",
        first_response: "8 6",
    };

    const performancePromisePalette = ["#6fd0ff", "#f3c969", "#7ddc9b", "#c7a6ff", "#ff9f7a", "#ff8fb3", "#9ee6d4", "#8fc7ff"];
    const seriesShadeSteps = {
        2: [0.18, -0.16],
        3: [0.22, 0, -0.18],
        4: [0.26, 0.08, -0.12, -0.28],
        5: [0.3, 0.14, 0, -0.16, -0.32],
        6: [0.34, 0.2, 0.06, -0.08, -0.22, -0.36],
        7: [0.36, 0.24, 0.12, 0, -0.12, -0.24, -0.36],
        8: [0.38, 0.27, 0.16, 0.05, -0.07, -0.18, -0.29, -0.4],
    };

    const riskMetricLabels = [
        ["maintainability_risk", "Maintainability"],
        ["change_risk", "Change"],
        ["performance_risk", "Performance"],
        ["correctness_risk", "Correctness"],
        ["architectural_risk", "Architecture"],
    ];

    const escapeHatchPatternWeights = {
        unsafe_block: 10,
        unsafe_fn: 10,
        unsafe_impl: 10,
        unsafe_trait: 10,
        extern_block: 8,
        extern_fn: 7,
        static_mut: 14,
        union: 12,
        raw_borrow: 6,
        asm_macro: 14,
        transmute: 12,
        maybe_uninit: 5,
        deref_impl: 4,
        deref_mut_impl: 5,
        glob_import: 2,
        container_ref_return: 3,
        repr_escape: 5,
        linkage_escape: 8,
        clippy_suppression: 3,
        lint_suppression: 2,
    };

    function byId(id) {
        return document.getElementById(id);
    }

    function escapeHtml(value) {
        return String(value ?? "")
            .replaceAll("&", "&amp;")
            .replaceAll("<", "&lt;")
            .replaceAll(">", "&gt;")
            .replaceAll('"', "&quot;")
            .replaceAll("'", "&#039;");
    }

    function riskClass(value, warn, bad) {
        if (value >= bad) {
            return "risk-bad";
        }
        if (value >= warn) {
            return "risk-warn";
        }
        return "risk-good";
    }

    function inverseRiskClass(value, warn, bad) {
        if (value <= bad) {
            return "risk-bad";
        }
        if (value <= warn) {
            return "risk-warn";
        }
        return "risk-good";
    }

    function clamp(value, min, max) {
        return Math.max(min, Math.min(max, value));
    }

    function metricCard(label, value) {
        return `<div class="metric-card"><span>${escapeHtml(label)}</span><strong>${escapeHtml(value)}</strong></div>`;
    }

    function activeProgressPill(run) {
        if (!run) {
            return `<div class="active-progress-pill"><span>Active Progress</span><strong>-</strong></div>`;
        }
        const progress = runProgress(run);
        const task = run.current_task_id || run.status || "-";
        const detail = run.current_task_detail ? `<em>${escapeHtml(run.current_task_detail)}</em>` : "";
        return `<div class="active-progress-pill ${run.status === "running" || run.status === "queued" ? "is-running" : ""}">
            <span>Active Progress</span>
            <strong>${progress.done}/${progress.total}</strong>
            <code>${escapeHtml(task)}</code>
            ${detail}
        </div>`;
    }

    function runButtonSelector(button) {
        if (!button) return "";
        if (button.dataset.run != null) return "all";
        if (button.dataset.runCategory) return `category/${button.dataset.runCategory}`;
        if (button.dataset.runItem) return `item/${button.dataset.runItem}`;
        return "";
    }

    function activeRunForSelector(selector) {
        if (!selector) return null;
        return [...state.runs].reverse().find((run) => {
            return run.selector === selector && activeRunStatuses.has(run.status);
        }) || null;
    }

    function renderRunButtonProgress(button, run) {
        if (!button) return;
        const label = button.dataset.runLabel || button.textContent.trim() || "Refresh";
        button.dataset.runLabel = label;
        if (!run) {
            clearButtonProgress(button, label);
            return;
        }

        const progress = runProgress(run);
        const current = run.current_task_id || (run.status === "queued" ? "Queued" : "Running");
        const percentLabel = progress.total ? `${progress.percent}%` : "Working";
        const description = progress.total
            ? `${progress.done} of ${progress.total} tasks complete`
            : "Refresh in progress";
        setButtonProgress(button, {
            label,
            meta: percentLabel,
            percent: progress.percent,
            task: current,
            description,
        });
    }

    function renderRunButtonsProgress() {
        document.querySelectorAll(".run-button[data-run], .run-button[data-run-category], .run-button[data-run-item]").forEach((button) => {
            const selector = button.dataset.runSelector || runButtonSelector(button);
            button.dataset.runSelector = selector;
            renderRunButtonProgress(button, activeRunForSelector(selector));
        });
    }

    function setButtonProgress(button, { label, meta, percent, task, description }) {
        if (!button) return;
        const normalizedPercent = Math.max(0, Math.min(100, Number(percent || 0)));
        button.dataset.runLabel = label;
        button.disabled = true;
        button.classList.add("is-progress");
        button.setAttribute("aria-busy", "true");
        button.setAttribute("aria-label", `${label}: ${description}`);
        button.innerHTML = `<span class="run-button__content">
                <span class="run-button__label">${escapeHtml(label)}</span>
                <span class="run-button__meta">${escapeHtml(meta)}</span>
            </span>
            <span class="run-button__progress" role="progressbar" aria-valuenow="${normalizedPercent}" aria-valuemin="0" aria-valuemax="100" aria-label="${escapeHtml(description)}">
                <span style="width:${normalizedPercent}%"></span>
            </span>
            ${task ? `<span class="run-button__task">${escapeHtml(task)}</span>` : ""}`;
    }

    function clearButtonProgress(button, label) {
        if (!button) return;
        button.disabled = false;
        button.classList.remove("is-progress");
        button.removeAttribute("aria-busy");
        button.removeAttribute("aria-describedby");
        button.removeAttribute("aria-label");
        button.innerHTML = escapeHtml(label);
    }

    function renderSummary(targetId, cards) {
        const target = byId(targetId);
        if (!target) return;
        target.innerHTML = cards.join("");
    }

    function renderTable(targetId, headers, rows) {
        const target = byId(targetId);
        if (!target) return;
        const head = headers.map((header) => `<th>${escapeHtml(header)}</th>`).join("");
        const body = rows.length
            ? rows.join("")
            : `<tr><td colspan="${headers.length}" class="muted">No data loaded.</td></tr>`;
        target.innerHTML = `<table><thead><tr>${head}</tr></thead><tbody>${body}</tbody></table>`;
    }

    function matchesFilter(item, query) {
        if (!query) {
            return true;
        }
        const text = JSON.stringify(item).toLowerCase();
        const tokens = String(query)
            .trim()
            .match(/"[^"]+"|\S+/g) || [];
        return tokens.every((rawToken) => {
            const token = rawToken.replace(/^"|"$/g, "");
            const scoped = token.match(/^([a-zA-Z_][\w.-]*):(>=|<=|>|<|=)?(.+)$/);
            if (!scoped) {
                return text.includes(token.toLowerCase()) || normalizeFilterText(text).includes(normalizeFilterText(token));
            }
            const [, rawField, rawOperator, rawExpected] = scoped;
            const field = rawField.toLowerCase();
            const operator = rawOperator || "=";
            const expected = rawExpected.replace(/^"|"$/g, "");
            const value = filterFieldValue(item, field);
            const numericValue = Number(value);
            const numericExpected = Number(expected);
            if (Number.isFinite(numericValue) && Number.isFinite(numericExpected) && operator !== "=") {
                if (operator === ">") return numericValue > numericExpected;
                if (operator === ">=") return numericValue >= numericExpected;
                if (operator === "<") return numericValue < numericExpected;
                if (operator === "<=") return numericValue <= numericExpected;
            }
            if (Number.isFinite(numericValue) && Number.isFinite(numericExpected) && operator === "=") {
                return numericValue === numericExpected;
            }
            const haystack = String(value ?? "").toLowerCase();
            return haystack.includes(expected.toLowerCase()) || normalizeFilterText(haystack).includes(normalizeFilterText(expected));
        });
    }

    function normalizeFilterText(value) {
        return String(value ?? "")
            .toLowerCase()
            .replaceAll("\\\\", "/")
            .replaceAll("\\", "/");
    }

    function filterFieldValue(item, field) {
        const aliases = {
            score: item.quality_score ?? item.score ?? item.escape_hatch_score ?? item.structural_risk ?? item.non_locality_risk ?? item.leverage_risk,
            risk: item.structural_risk ?? item.escape_hatch_score ?? item.non_locality_risk ?? item.locality_risk ?? item.leverage_risk ?? item.quality_score ?? item.score,
            quality: item.quality_score ?? item.score,
            fields: item.field_count,
            variants: item.variant_count,
            width: Math.max(Number(item.field_count || 0), Number(item.variant_count || 0)),
            module: item.module_key ?? item.module_name,
            file: item.path ?? item.name ?? item.file_path,
        };
        if (Object.prototype.hasOwnProperty.call(aliases, field)) {
            return aliases[field];
        }
        return field.split(".").reduce((value, part) => value?.[part], item);
    }

    function renderHotspots() {
        const query = byId("hotspots-filter").value;
        const filtered = state.hotspots.filter((item) => matchesFilter(item, query));
        const worst = state.hotspots[0];
        const files = new Set(state.hotspots.filter((item) => item.kind === "unit").map((item) => item.name));
        const largeFiles = state.hotspots.filter((item) => Number(item.sloc || 0) >= 150).length;

        renderSummary("hotspots-summary", [
            metricCard("Records", state.hotspots.length),
            metricCard("Files", files.size),
            metricCard("Worst quality", worst ? formatNumber.format(qualityScore(worst)) : "-"),
            metricCard("Large items", largeFiles),
            metricCard("Worst item", worst ? worst.name.split(/[\\/]/).pop() : "-"),
        ]);

        renderTable(
            "hotspots-table",
            ["Rank", "Kind", "Name", "Quality", "Score Drivers", "Cog", "Cyc", "MI", "Effort", "Density", "Bugs", "SLOC", "Signals"],
            filtered.map((item, index) => {
                const score = qualityScore(item);
                const scoreClass = riskClass(score, 300, 600);
                const miClass = inverseRiskClass(Number(item.mi || 0), 60, 40);
                return `<tr>
                    <td>${index + 1}</td>
                    <td><span class="pill">${escapeHtml(item.kind)}</span></td>
                    <td><code>${escapeHtml(item.name)}</code><div class="muted">line ${escapeHtml(item.start_line)}</div></td>
                    <td class="${scoreClass}">${formatNumber.format(score)}</td>
                    <td>${renderQualityComponentBar(item)}</td>
                    <td>${formatNumber.format(item.cognitive)}</td>
                    <td>${formatNumber.format(item.cyclomatic)}</td>
                    <td class="${miClass}">${formatNumber.format(item.mi)}</td>
                    <td>${formatNumber.format(item.effort || 0)}</td>
                    <td>${formatNumber.format(item.complexity_density || 0)}<div class="muted">ABC ${formatNumber.format(item.abc_density || 0)}</div></td>
                    <td>${formatNumber.format(item.bugs || 0)}</td>
                    <td>${formatNumber.format(item.sloc)}</td>
                    <td>${renderPills(item.signals)}</td>
                </tr>`;
            })
        );
    }

    function qualityScore(item) {
        return Number(item.quality_score ?? item.score ?? 0);
    }

    function qualityComponentScores(item) {
        const cognitive = Number(item.cognitive_score ?? Math.min(260, Number(item.cognitive || 0) * 3.7));
        const cyclomatic = Number(item.cyclomatic_score ?? Math.min(220, Number(item.cyclomatic || 0) * 2));
        const maintainability = Number(item.maintainability_score ?? Math.min(150, Math.max(0, 65 - Number(item.mi || 0)) * 1.2));
        const effort = Number(item.effort_score ?? Math.min(60, Math.log1p(Math.max(0, Number(item.effort || 0))) * 4));
        return { cognitive, cyclomatic, maintainability, effort };
    }

    function renderQualityComponentBar(item) {
        const components = qualityComponentScores(item);
        const max = Math.max(1, components.cognitive + components.cyclomatic + components.maintainability + components.effort);
        const parts = [
            ["cognitive", "Cog", components.cognitive],
            ["cyclomatic", "Cyc", components.cyclomatic],
            ["maintainability", "MI", components.maintainability],
            ["effort", "Effort", components.effort],
        ];
        return `<div class="quality-component-bar" title="${escapeHtml(parts.map(([, label, value]) => `${label} ${formatNumber.format(value)}`).join(" · "))}">
            <div class="quality-component-bar__track">
                ${parts.map(([key, label, value]) => `<i class="quality-component-bar__part quality-component-bar__part--${key}" style="width:${(value / max) * 100}%" aria-label="${escapeHtml(label)} ${formatNumber.format(value)}"></i>`).join("")}
            </div>
            <span>${parts.map(([, label, value]) => `${label} ${formatNumber.format(value)}`).join(" · ")}</span>
        </div>`;
    }

    function typeHealthRisk(item) {
        return Number(item?.structural_risk ?? 0);
    }

    function normalizedRisk(value, warn, bad) {
        const score = Number(value || 0);
        if (score <= 0) return 0;
        if (score <= warn) return clamp((score / Math.max(warn, 1)) * 40, 0, 40);
        if (score <= bad) return 40 + ((score - warn) / Math.max(bad - warn, 1)) * 40;
        return clamp(80 + ((score - bad) / Math.max(bad, 1)) * 20, 80, 100);
    }

    function categoryRisk(items, scoreFn, warn, bad) {
        if (!items.length) return 0;
        const scores = items.map((item) => normalizedRisk(scoreFn(item), warn, bad));
        const top = [...scores].sort((a, b) => b - a).slice(0, Math.min(5, scores.length));
        return mean(top) || 0;
    }

    function empiricalThresholds(values, fallbackWarn, fallbackBad) {
        const scores = values.map(Number).filter(Number.isFinite).sort((a, b) => a - b);
        if (scores.length < 8) {
            return { warn: fallbackWarn, bad: fallbackBad };
        }
        const warn = Math.max(1, percentileValue(scores, 0.9) || fallbackWarn);
        const bad = Math.max(warn + 1, percentileValue(scores, 0.98) || fallbackBad);
        return { warn: Math.round(warn * 100) / 100, bad: Math.round(bad * 100) / 100 };
    }

    function runMetricDelta(metricKey, currentValue) {
        const series = runMetricSeries(metricKey);
        if (series.length < 2) return "";
        const previous = Number(series[series.length - 2] || 0);
        const delta = Number(currentValue || 0) - previous;
        if (!delta) return "0 since last run";
        return `${delta > 0 ? "+" : ""}${formatNumber.format(delta)} since last run`;
    }

    function qualityOverviewMetric(label, value, action = null) {
        const data = action
            ? ` role="button" tabindex="0" data-quality-drill-dataset="${escapeHtml(action.dataset)}" data-quality-drill-filter="${escapeHtml(action.filter || "")}"`
            : "";
        return `<div class="quality-overview-metric ${action ? "quality-overview-metric--action" : ""}"${data}>
            <span>${escapeHtml(label)}</span>
            <strong>${escapeHtml(value)}</strong>
        </div>`;
    }

    function escapeHatchFieldCount(item, key) {
        return Number(item?.[key] ?? item?.counts?.[key] ?? 0);
    }

    function renderQualityOverview() {
        const target = byId("quality-overview");
        if (!target) return;

        const hotspots = state.hotspots || [];
        const clones = state.clones || [];
        const typeHealth = state.typeHealth || [];
        const locality = state.locality || [];
        const leverage = state.leverage || [];
        const escapeHatches = state.escapeHatches || [];
        const worstHotspot = hotspots.reduce((worst, item) => {
            if (!worst) return item;
            return qualityScore(item) > qualityScore(worst) ? item : worst;
        }, null);
        const hotspotFiles = new Set(hotspots.filter((item) => item.kind === "unit").map((item) => item.name));
        const largeHotspots = hotspots.filter((item) => Number(item.sloc || 0) >= 150).length;
        const cloneInstances = clones.reduce((sum, item) => sum + ((item.instances || []).length), 0);
        const cloneGroups = clones.length;
        const crossFileClones = clones.filter((item) => (item.file_count || 0) >= 2).length;
        const astClones = clones.filter((item) => item.engine === "ast").length;
        const widestClone = clones.reduce((max, item) => Math.max(max, item.max_line_span || 0), 0);
        const worstType = typeHealth.reduce((worst, item) => {
            if (!worst) return item;
            return typeHealthRisk(item) > typeHealthRisk(worst) ? item : worst;
        }, null);
        const structuralRiskTypes = typeHealth.filter((item) => typeHealthRisk(item) >= 40).length;
        const wideStructs = typeHealth.filter((item) => (item.field_count || 0) >= 16).length;
        const implSpreadTypes = typeHealth.filter((item) => (item.impl_file_count || 0) >= 4).length;
        const escapeHatchModules = escapeHatches.length;
        const escapeHatchTotal = escapeHatches.reduce((sum, item) => sum + Number(item.total_count || 0), 0);
        const unsafeModules = escapeHatches.filter((item) => Number(item.unsafe_count || 0) > 0).length;
        const allowAttributes = escapeHatches.reduce((sum, item) => sum + escapeHatchFieldCount(item, "allow_attribute_count"), 0);
        const clippyAllows = escapeHatches.reduce((sum, item) => sum + escapeHatchFieldCount(item, "clippy_allow_count"), 0);
        const derefCoercions = escapeHatches.reduce((sum, item) => sum + Number(item.deref_coercion_count || 0), 0);
        const globImports = escapeHatches.reduce((sum, item) => sum + Number(item.glob_import_count || 0), 0);
        const containerRefs = escapeHatches.reduce((sum, item) => sum + Number(item.container_ref_return_count || 0), 0);
        const avgCodeLocality = mean(locality.map(i => i.locality_score)) || 0;
        const avgNonLocalityRisk = mean(locality.map(localityRisk)) || 0;
        const farDependencyModules = locality.filter((item) => (item.far_dependencies || 0) > 0).length;
        const modulesWithoutNearbyTests = locality.filter((item) => item.test_locality === "none").length;
        const hiddenCouplingModules = locality.filter((item) => (item.hidden_coupling_count || 0) > 0).length;
        const avgLeverageScore = mean(leverage.map(i => i.leverage_score ?? i.total_leverage_score)) || 0;
        const avgLeverageRisk = mean(leverage.map(leverageRisk)) || 0;
        const broadReachModules = leverage.filter((item) => (item.reach || 0) >= 5).length;
        const divergenceModules = leverage.filter((item) => (item.divergence_count || 0) > 0).length;
        const risks = {
            maintainability: categoryRisk(hotspots, qualityScore, 300, 600),
            structure: categoryRisk(typeHealth, typeHealthRisk, 40, 70),
            duplication: categoryRisk(clones, (item) => Number(item.score || 0), 20, 40),
            escapeHatches: categoryRisk(escapeHatches, (item) => Number(item.escape_hatch_score || 0), 20, 50),
            locality: categoryRisk(locality, localityRisk, 15, 30),
            leverage: categoryRisk(leverage, leverageRisk, 20, 40),
        };

        const groups = [
            {
                cls: "maintainability",
                title: "Maintainability",
                risk: risks.maintainability,
                summary: `${formatNumber.format(risks.maintainability)} category risk${runMetricDelta("quality_risk_count", hotspots.filter((item) => qualityScore(item) >= 300).length) ? ` · ${runMetricDelta("quality_risk_count", hotspots.filter((item) => qualityScore(item) >= 300).length)}` : ""}`,
                metrics: [
                    { label: "Records", value: hotspots.length },
                    { label: "Files", value: hotspotFiles.size },
                    { label: "Worst quality", value: worstHotspot ? formatNumber.format(qualityScore(worstHotspot)) : "-" },
                    { label: "Large items", value: largeHotspots, action: { dataset: "hotspots", filter: "sloc:>=150" } },
                    { label: "Worst item", value: worstHotspot ? worstHotspot.name.split(/[\\/]/).pop() : "-" },
                ],
            },
            {
                cls: "structure",
                title: "Structure",
                risk: risks.structure,
                summary: `${formatNumber.format(risks.structure)} category risk`,
                metrics: [
                    { label: "Types", value: typeHealth.length },
                    { label: "High risk", value: structuralRiskTypes, action: { dataset: "typeHealth", filter: "risk:>=40" } },
                    { label: "Wide structs", value: wideStructs, action: { dataset: "typeHealth", filter: "fields:>=16" } },
                    { label: "Impl spread", value: implSpreadTypes, action: { dataset: "typeHealth", filter: "impl_file_count:>=4" } },
                    { label: "Worst type", value: worstType ? worstType.type_name : "-" },
                ],
            },
            {
                cls: "duplication",
                title: "Duplication",
                risk: risks.duplication,
                summary: `${formatNumber.format(risks.duplication)} category risk`,
                metrics: [
                    { label: "Clone Groups", value: cloneGroups },
                    { label: "Total Instances", value: cloneInstances },
                    { label: "Avg Instances", value: cloneGroups ? (cloneInstances / cloneGroups).toFixed(1) : "-" },
                    { label: "Cross-file Groups", value: crossFileClones, action: { dataset: "clones", filter: "file_count:>=2" } },
                    { label: "AST Groups", value: astClones, action: { dataset: "clones", filter: "engine:ast" } },
                    { label: "Widest Span", value: widestClone ? `${widestClone} lines` : "-" },
                ],
            },
            {
                cls: "escape-hatches",
                title: "Escape Hatches",
                risk: risks.escapeHatches,
                summary: `${formatNumber.format(risks.escapeHatches)} category risk`,
                metrics: [
                    { label: "Modules", value: escapeHatchModules },
                    { label: "Total Uses", value: escapeHatchTotal },
                    { label: "Unsafe Modules", value: unsafeModules, action: { dataset: "escapeHatches", filter: "unsafe_count:>0" } },
                    { label: "Deref/DerefMut", value: derefCoercions },
                    { label: "Glob Imports", value: globImports },
                    { label: "Container Refs", value: containerRefs },
                    { label: "Allow/Expect", value: allowAttributes, action: { dataset: "escapeHatches", filter: "allow_attribute_count:>0" } },
                    { label: "Clippy Allow/Expect", value: clippyAllows, action: { dataset: "escapeHatches", filter: "clippy_allow_count:>0" } },
                ],
            },
            {
                cls: "locality",
                title: "Code Locality",
                risk: risks.locality,
                summary: `${formatNumber.format(risks.locality)} category risk`,
                metrics: [
                    { label: "Avg Score", value: formatNumber.format(avgCodeLocality) },
                    { label: "Avg Risk", value: formatNumber.format(avgNonLocalityRisk) },
                    { label: "Far Dependencies", value: farDependencyModules, action: { dataset: "locality", filter: "far_dependencies:>0" } },
                    { label: "Hidden Coupling", value: hiddenCouplingModules, action: { dataset: "locality", filter: "hidden_coupling_count:>0" } },
                    { label: "No Nearby Tests", value: modulesWithoutNearbyTests, action: { dataset: "locality", filter: "test_locality:none" } },
                ],
            },
            {
                cls: "leverage",
                title: "Leverage",
                risk: risks.leverage,
                summary: `${formatNumber.format(risks.leverage)} category risk`,
                metrics: [
                    { label: "Avg Score", value: formatNumber.format(avgLeverageScore) },
                    { label: "Avg Risk", value: formatNumber.format(avgLeverageRisk) },
                    { label: "Broad Reach", value: broadReachModules, action: { dataset: "leverage", filter: "reach:>=5" } },
                    { label: "Divergence", value: divergenceModules, action: { dataset: "leverage", filter: "divergence_count:>0" } },
                ],
            },
        ];

        target.innerHTML = groups.map((group) => `<section class="quality-overview-card quality-overview-card--${group.cls}">
            <div class="quality-overview-card__header">
                <span class="quality-overview-card__marker"></span>
                <div>
                    <h3>${escapeHtml(group.title)}</h3>
                    <p>${escapeHtml(group.summary)}</p>
                </div>
                <strong class="${riskClass(group.risk, 40, 80)}">${formatNumber.format(group.risk)}</strong>
            </div>
            <div class="quality-overview-card__metrics">
                ${group.metrics.map((metric) => qualityOverviewMetric(metric.label, metric.value, metric.action)).join("")}
            </div>
        </section>`).join("");
        target.querySelectorAll("[data-quality-drill-dataset]").forEach((metric) => {
            metric.addEventListener("click", () => drillToQualityDataset(metric.dataset.qualityDrillDataset, metric.dataset.qualityDrillFilter || ""));
            metric.addEventListener("keydown", (event) => {
                if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    drillToQualityDataset(metric.dataset.qualityDrillDataset, metric.dataset.qualityDrillFilter || "");
                }
            });
        });
    }

    function normalizePath(value) {
        return String(value || "").replaceAll("\\", "/").toLowerCase();
    }

    function renderClones() {
        const query = byId("clones-filter").value;
        const filtered = state.clones.filter((item) => matchesFilter(item, query));
        const totalInstances = state.clones.reduce((sum, item) => sum + (item.instances || []).length, 0);
        const crossFileCount = state.clones.filter((item) => (item.file_count || 0) >= 2).length;
        const widest = state.clones.reduce((max, item) => Math.max(max, item.max_line_span || 0), 0);
        const astCount = state.clones.filter((item) => item.engine === "ast").length;

        renderSummary("clones-summary", [
            metricCard("Clone Groups", state.clones.length),
            metricCard("Total Instances", totalInstances),
            metricCard("Avg Instances", state.clones.length ? (totalInstances / state.clones.length).toFixed(1) : "-"),
            metricCard("Cross-file Groups", crossFileCount),
            metricCard("AST Groups", astCount),
            metricCard("Widest Span", widest ? `${widest} lines` : "-"),
        ]);

        renderTable(
            "clones-table",
            ["Engine", "Group Hash", "Instances", "Files", "Score", "Density", "Token Count", "Signals", "Locations", "Preview"],
            filtered.map((item) => {
                const hash = item.hash || item.group_hash || "";
                const instances = item.instances || [];
                const locations = instances.map((inst) =>
                    `<div><code>${escapeHtml(inst.file_path)}:${inst.start_line}-${inst.end_line}</code></div>`
                ).join("");
                const scoreClass = riskClass(item.score || 0, 20, 40);
                const preview = instances.find((inst) => inst.snippet)?.snippet || "";

                return `<tr>
                    <td><span class="pill">${escapeHtml(item.engine || "token")}</span></td>
                    <td><code>${escapeHtml(hash.substring(0, 8))}</code></td>
                    <td>${instances.length}</td>
                    <td>${item.file_count ?? "-"}</td>
                    <td class="${scoreClass}">${formatNumber.format(item.score)}</td>
                    <td>${formatNumber.format(cloneDensity(item))}</td>
                    <td>${item.token_count}</td>
                    <td>${renderPills(item.signals)}</td>
                    <td class="small-text">${locations}</td>
                    <td class="small-text">${preview ? `<details class="clone-snippet-details"><summary>Snippet</summary><pre>${escapeHtml(preview)}</pre></details>` : "-"}</td>
                </tr>`;
            })
        );
    }

    function renderTypeHealth() {
        const query = byId("type-health-filter")?.value || "";
        const rows = state.typeHealth || [];
        const filtered = rows.filter((item) => matchesFilter(item, query));
        renderTable(
            "type-health-table",
            ["Rank", "Kind", "Type", "Path", "Risk", "Width", "Methods", "Impls", "Impl Files", "Span", "Signals"],
            filtered.map((item, index) => {
                const risk = typeHealthRisk(item);
                const scoreClass = riskClass(risk, 40, 70);
                const width = Math.max(Number(item.field_count || 0), Number(item.variant_count || 0));
                return `<tr>
                    <td>${index + 1}</td>
                    <td><span class="pill">${escapeHtml(item.kind || "type")}</span></td>
                    <td><code>${escapeHtml(item.qualified_name || item.type_name)}</code><div class="muted">line ${escapeHtml(item.line || "-")}</div></td>
                    <td><code>${escapeHtml(item.path || "")}</code></td>
                    <td class="${scoreClass}">${formatNumber.format(risk)}</td>
                    <td>${formatNumber.format(width)}</td>
                    <td>${formatNumber.format(item.method_count || 0)}</td>
                    <td>${formatNumber.format(item.impl_block_count || 0)}</td>
                    <td>${formatNumber.format(item.impl_file_count || 0)}</td>
                    <td>${formatNumber.format(item.declaration_span || 0)}</td>
                    <td>${renderPills(item.signals)}</td>
                </tr>`;
            })
        );
    }

    function renderEscapeHatches() {
        const query = byId("escape-hatches-filter")?.value || "";
        const rows = state.escapeHatches || [];
        const filtered = rows.filter((item) => matchesFilter(item, query));
        const totalUses = rows.reduce((sum, item) => sum + Number(item.total_count || 0), 0);
        const escapeWeight = (item, keys, fallbackCountKey, fallbackWeight) => {
            const counts = item.counts || {};
            const weighted = keys.reduce((sum, key) => sum + Number(counts[key] || 0) * escapeHatchPatternWeights[key], 0);
            return weighted || Number(item[fallbackCountKey] || 0) * fallbackWeight;
        };
        const totals = {
            unsafe: rows.reduce((sum, item) => sum + Number(item.unsafe_count || 0), 0),
            ffi: rows.reduce((sum, item) => sum + Number(item.ffi_count || 0), 0),
            globals: rows.reduce((sum, item) => sum + Number(item.global_mutability_count || 0), 0),
            raw: rows.reduce((sum, item) => sum + Number(item.raw_memory_count || 0), 0),
            deref: rows.reduce((sum, item) => sum + Number(item.deref_coercion_count || 0), 0),
            glob: rows.reduce((sum, item) => sum + Number(item.glob_import_count || 0), 0),
            containerRefs: rows.reduce((sum, item) => sum + Number(item.container_ref_return_count || 0), 0),
            layout: rows.reduce((sum, item) => sum + Number(item.layout_linkage_count || 0), 0),
            clippy: rows.reduce((sum, item) => sum + Number(item.clippy_suppression_count || 0), 0),
            lint: rows.reduce((sum, item) => sum + Number(item.lint_suppression_count || 0), 0),
            allows: rows.reduce((sum, item) => sum + escapeHatchFieldCount(item, "allow_attribute_count"), 0),
            clippyAllows: rows.reduce((sum, item) => sum + escapeHatchFieldCount(item, "clippy_allow_count"), 0),
        };
        const weightedTotals = {
            unsafe: rows.reduce((sum, item) => sum + escapeWeight(item, ["unsafe_block", "unsafe_fn", "unsafe_impl", "unsafe_trait"], "unsafe_count", 10), 0),
            ffi: rows.reduce((sum, item) => sum + escapeWeight(item, ["extern_block", "extern_fn"], "ffi_count", 8), 0),
            globals: rows.reduce((sum, item) => sum + escapeWeight(item, ["static_mut"], "global_mutability_count", 14), 0),
            raw: rows.reduce((sum, item) => sum + escapeWeight(item, ["raw_borrow", "asm_macro", "transmute", "maybe_uninit", "union"], "raw_memory_count", 8), 0),
            deref: rows.reduce((sum, item) => sum + escapeWeight(item, ["deref_impl", "deref_mut_impl"], "deref_coercion_count", 4.5), 0),
            glob: rows.reduce((sum, item) => sum + escapeWeight(item, ["glob_import"], "glob_import_count", 2), 0),
            containerRefs: rows.reduce((sum, item) => sum + escapeWeight(item, ["container_ref_return"], "container_ref_return_count", 3), 0),
            layout: rows.reduce((sum, item) => sum + escapeWeight(item, ["repr_escape", "linkage_escape"], "layout_linkage_count", 6.5), 0),
            clippy: rows.reduce((sum, item) => sum + escapeWeight(item, ["clippy_suppression"], "clippy_suppression_count", 3), 0),
            lint: rows.reduce((sum, item) => sum + escapeWeight(item, ["lint_suppression"], "lint_suppression_count", 2), 0),
            allows: totals.allows * 2,
            clippyAllows: totals.clippyAllows * 3,
        };
        const maxTotal = Math.max(1, ...Object.values(weightedTotals));
        const bars = [
            ["Unsafe", totals.unsafe, weightedTotals.unsafe],
            ["FFI", totals.ffi, weightedTotals.ffi],
            ["Global mutability", totals.globals, weightedTotals.globals],
            ["Raw memory", totals.raw, weightedTotals.raw],
            ["Deref/DerefMut", totals.deref, weightedTotals.deref],
            ["Glob imports", totals.glob, weightedTotals.glob],
            ["Container refs", totals.containerRefs, weightedTotals.containerRefs],
            ["Layout/linkage", totals.layout, weightedTotals.layout],
            ["Allow/expect attributes", totals.allows, weightedTotals.allows],
            ["Clippy allow/expect", totals.clippyAllows, weightedTotals.clippyAllows],
            ["Clippy suppressions", totals.clippy, weightedTotals.clippy],
            ["All lint suppressions", totals.lint, weightedTotals.lint],
        ];
        const overview = byId("escape-hatches-overview");
        if (overview) {
            overview.innerHTML = `<div class="escape-hatch-cards">
                ${metricCard("Modules", rows.length)}
                ${metricCard("Total uses", totalUses)}
                ${metricCard("Unsafe uses", totals.unsafe)}
                ${metricCard("Deref/DerefMut", totals.deref)}
                ${metricCard("Glob imports", totals.glob)}
                ${metricCard("Container refs", totals.containerRefs)}
                ${metricCard("Allow/expect", totals.allows)}
                ${metricCard("Clippy allow/expect", totals.clippyAllows)}
                ${metricCard("Clippy suppressions", totals.clippy)}
            </div>
            <div class="escape-hatch-bars">
                ${bars.map(([label, count, value]) => `<div class="escape-hatch-bar">
                    <span>${escapeHtml(label)}</span>
                    <div><i style="width:${(value / maxTotal) * 100}%"></i></div>
                    <strong>${formatNumber.format(value)}<small>${formatNumber.format(count)} uses</small></strong>
                </div>`).join("")}
            </div>`;
        }

        renderTable(
            "escape-hatches-table",
            ["Rank", "Module", "Score", "Total", "Unsafe", "FFI", "Globals", "Raw", "Deref", "Glob", "Container Refs", "Layout", "Allow/Expect", "Clippy Allow/Expect", "Clippy Supp.", "Allow/Expect Lines", "Locations", "Signals"],
            filtered.map((item, index) => {
                const locations = (item.locations || [])
                    .slice(0, 8)
                    .map((location) => `<code>${escapeHtml(location.label)}:${escapeHtml(location.line)}</code>`)
                    .join(" ");
                const allowLocations = (item.allow_locations || [])
                    .slice(0, 8)
                    .map((location) => `<code title="${escapeHtml(location.snippet || "")}">allow/expect:${escapeHtml(location.line)}</code>`)
                    .join(" ");
                const score = Number(item.escape_hatch_score || 0);
                return `<tr>
                    <td>${index + 1}</td>
                    <td><code>${escapeHtml(item.module_key || item.module_name)}</code><div class="muted">${escapeHtml(item.path || "")}</div></td>
                    <td class="${riskClass(score, 20, 50)}">${formatNumber.format(score)}</td>
                    <td>${formatNumber.format(item.total_count || 0)}</td>
                    <td>${formatNumber.format(item.unsafe_count || 0)}</td>
                    <td>${formatNumber.format(item.ffi_count || 0)}</td>
                    <td>${formatNumber.format(item.global_mutability_count || 0)}</td>
                    <td>${formatNumber.format(item.raw_memory_count || 0)}</td>
                    <td>${formatNumber.format(item.deref_coercion_count || 0)}</td>
                    <td>${formatNumber.format(item.glob_import_count || 0)}</td>
                    <td>${formatNumber.format(item.container_ref_return_count || 0)}</td>
                    <td>${formatNumber.format(item.layout_linkage_count || 0)}</td>
                    <td>${formatNumber.format(escapeHatchFieldCount(item, "allow_attribute_count"))}</td>
                    <td>${formatNumber.format(escapeHatchFieldCount(item, "clippy_allow_count"))}</td>
                    <td>${formatNumber.format(item.clippy_suppression_count || 0)}</td>
                    <td class="small-text">${allowLocations || "-"}</td>
                    <td class="small-text">${locations || "-"}</td>
                    <td>${renderPills(item.signals || [])}</td>
                </tr>`;
            })
        );
    }

    function renderSearchSpeedCharts(items, container = null) {
        if (!container) {
            return;
        }
        if (!items.length) {
            container.innerHTML = `<section class="panel-card chart-panel"><div class="chart-empty">No search speed data matches the current filter.</div></section>`;
            return;
        }

        const scopeBuilders = {
            tabs: () => ({
                title: "Tabs Against Time",
                subtitle: "All-open-tabs aggregate-size scenarios. Solid = completion, dashed = first response.",
                hardLimitText: "No hard limit observed in the measured tab range.",
                series: buildSearchSpeedSeries(
                    items,
                    (item) => item.mode === "all" && item.scaling_axis === "aggregate_size",
                    (item) => item.latency_kind,
                    (key) => ({
                        label: latencyLabel(key),
                        shortLabel: latencyLabel(key),
                        latencyKind: key,
                        color: searchLatencyColors[key] || "#6fd0ff",
                        dasharray: searchLatencyDash[key] || "",
                        order: key === "completion" ? 0 : 1,
                    })
                ),
                insightsFor: buildAggregateScopeInsights,
            }),
            files: () => ({
                title: "Files Against Time",
                subtitle: "Current-workspace aggregate-size scenarios. Solid = completion, dashed = first response.",
                hardLimitText: "No hard limit observed in the measured file-count range.",
                series: buildSearchSpeedSeries(
                    items,
                    (item) => item.mode === "current" && item.scaling_axis === "aggregate_size",
                    (item) => item.latency_kind,
                    (key) => ({
                        label: latencyLabel(key),
                        shortLabel: latencyLabel(key),
                        latencyKind: key,
                        color: searchLatencyColors[key] || "#6fd0ff",
                        dasharray: searchLatencyDash[key] || "",
                        order: key === "completion" ? 0 : 1,
                    })
                ),
                insightsFor: buildAggregateScopeInsights,
            }),
            fileSize: () => ({
                title: "File Size Against Time",
                subtitle: "Active, Current, and All file-size scenarios. Color = mode, dashed = first response.",
                hardLimitText: "No hard limit observed; every file-size series completed its largest sampled input.",
                series: buildSearchSpeedSeries(
                    items,
                    (item) => item.scaling_axis === "file_size",
                    (item) => `${item.mode}:${item.latency_kind}`,
                    (key) => {
                        const [mode, latencyKind] = key.split(":");
                        const modeLabel = titleCase(mode);
                        const latencyText = latencyKind === "first_response" ? "First response" : "Completion";
                        const latencyOrder = latencyKind === "completion" ? 0 : 1;
                        const modeOrder = { active: 0, current: 1, all: 2 }[mode] ?? 9;
                        return {
                            label: `${modeLabel} ${latencyText}`,
                            shortLabel: modeLabel,
                            mode,
                            latencyKind,
                            color: searchModeColors[mode] || "#6fd0ff",
                            dasharray: searchLatencyDash[latencyKind] || "",
                            order: modeOrder * 2 + latencyOrder,
                        };
                    }
                ),
                insightsFor: buildFileSizeInsights,
            }),
        };
        const scopeKey = scopeBuilders[state.searchChartScope] ? state.searchChartScope : "tabs";
        const chart = scopeBuilders[scopeKey]();
        const scopeToggle = `<div class="segmented-control segmented-control--compact" role="group" aria-label="Search chart scope">
            ${[
            ["tabs", "Tabs"],
            ["files", "Files"],
            ["fileSize", "File size"],
        ].map(([key, label]) => `<button type="button" class="${scopeKey === key ? "is-active" : ""}" aria-pressed="${scopeKey === key ? "true" : "false"}" data-search-chart-scope="${key}">${label}</button>`).join("")}
        </div>`;

        container.innerHTML = [
            buildSearchSpeedLineCard({
                title: chart.title,
                subtitle: chart.subtitle,
                series: chart.series,
                insights: chart.insightsFor(chart.series),
                hardLimitText: chart.hardLimitText,
                controls: scopeToggle,
            }),
        ].join("");
        container.querySelectorAll("[data-search-chart-scope]").forEach((button) => {
            button.addEventListener("click", () => {
                state.searchChartScope = button.dataset.searchChartScope || "tabs";
                renderSearchSpeedCharts(items);
            });
        });
    }

    function renderPerformanceHeadlineCharts() {
        const target = byId("performance-headline-charts");
        const data = buildPerformanceAnswerChainData();
        if (target) {
            target.innerHTML = [
                renderPerformanceScatterPanel({
                    id: "pressure-scaling",
                    title: "Latency Cost × Load",
                    caption: "Each dot is a measured load point. X is normalized load; Y is latency per unit compared with that series' first point.",
                    points: data.pressureGrowth,
                    empty: "No rows have comparable budget-pressure load steps.",
                    x: { label: "Load / max measured load", min: 0, max: data.pressureBounds.xMax, threshold: 1, valueLabel: formatRatio },
                    y: { label: "Latency per unit vs baseline", min: 0, max: data.pressureBounds.yMax, threshold: 1, valueLabel: formatRatio },
                    sideTitle: "Worst latency cost points",
                    quadrants: [
                        { x: "left", y: "bottom", tone: "good", title: "Small + linear", detail: "low load, stable cost" },
                        { x: "right", y: "bottom", tone: "local", title: "Large + linear", detail: "high load, stable cost" },
                        { x: "left", y: "top", tone: "architecture", title: "Early drift", detail: "cost per unit rising" },
                        { x: "right", y: "top", tone: "triage", title: "Falloff", detail: "large + nonlinear" },
                    ],
                }),
                renderPerformanceScatterPanel({
                    id: "memory-elapsed",
                    title: "Resource Cost × Load",
                    caption: "Each dot is a measured load point. X is normalized load; Y is resource cost per unit compared with that series' first point.",
                    points: data.memoryElapsed,
                    empty: "No resource rows with comparable load steps loaded.",
                    x: { label: "Load / max measured load", min: 0, max: data.memoryBounds.xMax, threshold: 1, valueLabel: formatRatio },
                    y: { label: "Resource cost per unit vs baseline", min: 0, max: data.memoryBounds.yMax, threshold: 1, valueLabel: formatRatio },
                    sideTitle: "Worst resource cost points",
                }),
                renderPerformanceRiskRegisterPanel(),
                renderPerformanceDistributionGlyph({
                    id: "budget",
                    title: "Budget Pressure",
                    caption: "Mean latency divided by the row budget. Target is 1.0x.",
                    items: data.budgetPressure,
                    empty: "No latency rows with budgets loaded.",
                    bounds: { min: 0, max: 2, scale: "linear" },
                    markers: [
                        { value: 0.7, kind: "warn", label: "watch" },
                        { value: 1, kind: "bad", label: "over" },
                    ],
                    buckets: [
                        { cls: "good", label: "< 0.7 healthy", test: (item) => item.value < 0.7 },
                        { cls: "warn", label: "0.7-1.0 watch", test: (item) => item.value >= 0.7 && item.value < 1 },
                        { cls: "bad", label: ">= 1.0 over", test: (item) => item.value >= 1 },
                    ],
                    driverFor: (item) => item.resource || "cpu",
                    valueLabel: formatRatio,
                    rowFor: renderPerformanceMetricRow,
                }),
                renderPerformanceDistributionGlyph({
                    id: "scaling",
                    title: "Scaling Growth",
                    caption: "Time multiplier when the workload doubles. 2.0x is linear.",
                    items: data.scalingGrowth,
                    empty: "No scale series with at least two points loaded.",
                    bounds: { min: 0.8, max: 4, scale: "log" },
                    markers: [
                        { value: 1.5, kind: "warn", label: "good" },
                        { value: 2.2, kind: "bad", label: "super-linear" },
                    ],
                    buckets: [
                        { cls: "good", label: "flat < 1.2", test: (item) => item.value < 1.2 },
                        { cls: "good", label: "sub-linear 1.2-1.8", test: (item) => item.value >= 1.2 && item.value < 1.8 },
                        { cls: "warn", label: "linear 1.8-2.2", test: (item) => item.value >= 1.8 && item.value < 2.2 },
                        { cls: "bad", label: "super-linear >= 2.2", test: (item) => item.value >= 2.2 },
                    ],
                    driverFor: (item) => item.family || "unmapped",
                    valueLabel: formatRatio,
                    rowFor: renderPerformanceMetricRow,
                }),
                renderPerformanceDistributionGlyph({
                    id: "capacity",
                    title: "Capacity Headroom",
                    caption: "Capacity ceiling against the matched promise target. Vertical guide is 1.0x.",
                    items: data.capacityHeadroom,
                    empty: "No capacity scenarios loaded.",
                    bounds: { min: 0, max: Math.max(2, ...data.capacityHeadroom.map((item) => item.value || 0)), scale: "linear" },
                    markers: [{ value: 1, kind: "target", label: "target" }],
                    shape: "strip",
                    buckets: [
                        { cls: "bad", label: "failed below target", test: (item) => item.failed && item.value < 1 },
                        { cls: "warn", label: "failed past target", test: (item) => item.failed && item.value >= 1 },
                        { cls: "good", label: "proven without ceiling", test: (item) => !item.failed },
                    ],
                    driverFor: (item) => item.failureMode || "not_reached",
                    valueLabel: formatRatio,
                    rowFor: renderPerformanceMetricRow,
                    worstSort: (left, right) => (right.failed ? 1 : 0) - (left.failed ? 1 : 0) || left.value - right.value,
                }),
                renderPerformanceDistributionGlyph({
                    id: "resources",
                    title: "Resource Intensity",
                    caption: "Peak memory divided by workload size. Guide is a derived 2x-median budget.",
                    items: data.resourceIntensity,
                    empty: "No resource intensity rows loaded.",
                    bounds: data.resourceBounds,
                    markers: data.resourceThreshold ? [{ value: data.resourceThreshold, kind: "target", label: "2x median" }] : [],
                    buckets: [
                        { cls: "good", label: "< median", test: (item) => item.value < data.resourceMedian },
                        { cls: "warn", label: "median-2x", test: (item) => item.value >= data.resourceMedian && item.value < data.resourceThreshold },
                        { cls: "bad", label: ">= 2x median", test: (item) => item.value >= data.resourceThreshold },
                    ],
                    driverFor: (item) => item.family || item.resource || "resource",
                    valueLabel: formatBytes,
                    rowFor: renderPerformanceMetricRow,
                }),
            ].join("");
            attachPerformanceScatterHandlers(target);
            attachPerformanceDistributionHandlers(target);
        }
        renderPerformanceFocusList(data.pressureGrowth);
    }

    function renderPerformanceRiskRegisterPanel() {
        return `<aside class="panel-card performance-focus-panel">
            <div class="panel-card__header">
                <div>
                    <h2>Risk register</h2>
                    <p>Ranked by combined budget pressure and scaling growth.</p>
                </div>
            </div>
            <div id="performance-focus-list" class="performance-focus-list"></div>
        </aside>`;
    }

    function labeledPerformanceRows(rows, sourceLabel) {
        return uniquePerformanceRows(rows).map((item) => ({ ...item, sourceLabel }));
    }

    function performancePromiseColor(key) {
        const scenarios = state.performanceReview?.scenarios || [];
        const index = scenarios.findIndex((scenario) => scenario.id === key);
        if (index >= 0) return performancePromisePalette[index % performancePromisePalette.length];
        const hash = String(key || "").split("").reduce((value, char) => ((value << 5) - value + char.charCodeAt(0)) | 0, 0);
        return performancePromisePalette[Math.abs(hash) % performancePromisePalette.length];
    }

    function parseHexColor(color) {
        const match = String(color || "").trim().match(/^#([0-9a-f]{6})$/i);
        if (!match) return null;
        const numeric = Number.parseInt(match[1], 16);
        return {
            r: (numeric >> 16) & 255,
            g: (numeric >> 8) & 255,
            b: numeric & 255,
        };
    }

    function rgbToHex({ r, g, b }) {
        return `#${[r, g, b].map((value) => Math.round(Math.max(0, Math.min(255, value))).toString(16).padStart(2, "0")).join("")}`;
    }

    function shadeSeriesColor(baseColor, index, total) {
        if (total <= 1) return baseColor;
        const steps = seriesShadeSteps[Math.min(total, 8)] || seriesShadeSteps[8];
        const step = steps[index % steps.length] || 0;
        const base = parseHexColor(baseColor);
        if (!base) return baseColor;
        const target = step >= 0 ? { r: 255, g: 255, b: 255 } : { r: 0, g: 0, b: 0 };
        const amount = Math.abs(step);
        return rgbToHex({
            r: base.r + (target.r - base.r) * amount,
            g: base.g + (target.g - base.g) * amount,
            b: base.b + (target.b - base.b) * amount,
        });
    }

    function withDistinctSeriesColors(series) {
        const groups = new Map();
        (series || []).forEach((entry, index) => {
            const key = String(entry.color || "").toLowerCase();
            if (!key) return;
            if (!groups.has(key)) groups.set(key, []);
            groups.get(key).push(index);
        });
        return (series || []).map((entry, index) => {
            const group = groups.get(String(entry.color || "").toLowerCase()) || [];
            if (group.length <= 1) return entry;
            return {
                ...entry,
                color: shadeSeriesColor(entry.color, group.indexOf(index), group.length),
            };
        });
    }

    function normalisePromiseToken(value) {
        return String(value || "").toLowerCase().replace(/[^a-z0-9]+/g, "_").replace(/^_+|_+$/g, "");
    }

    function promiseEvidenceMatches(item, evidence) {
        const itemValues = [
            item?.id,
            item?.name,
            item?.label,
            item?.benchmark_key,
            item?.scenario_id,
            item?.scenario,
            item?.scenario_label,
        ].map(normalisePromiseToken).filter(Boolean);
        const evidenceValues = [
            evidence?.id,
            evidence?.label,
            evidence?.benchmark_key,
            evidence?.scenario,
            evidence?.scenario_label,
        ].map(normalisePromiseToken).filter(Boolean);
        return itemValues.some((itemValue) => evidenceValues.some((evidenceValue) => {
            return itemValue === evidenceValue || itemValue.startsWith(`${evidenceValue}_`) || evidenceValue.startsWith(`${itemValue}_`);
        }));
    }

    function performancePromiseForItem(item) {
        if (item?.promiseId) {
            return { id: item.promiseId, title: item.promiseTitle || item.promiseId, color: performancePromiseColor(item.promiseId) };
        }
        const scenarios = state.performanceReview?.scenarios || [];
        const family = item?.workload_family || item?.family;
        const familyToken = normalisePromiseToken(family);
        const itemTokens = [item?.id, item?.name, item?.label, item?.benchmark_key, item?.scenario, item?.scenario_label]
            .map(normalisePromiseToken)
            .filter(Boolean);
        const scored = scenarios
            .map((scenario) => {
                const scenarioToken = normalisePromiseToken(`${scenario.id || ""} ${scenario.title || ""}`);
                const exact = Object.values(scenario.evidence || {})
                    .some((rows) => Array.isArray(rows) && rows.some((row) => promiseEvidenceMatches(item, row)));
                const familyMatch = family && scenarioFamilies(scenario).includes(family);
                const tokenMatch = itemTokens.some((token) => token.includes(normalisePromiseToken(scenario.id)) || scenarioToken.includes(token));
                const familyScenarioMatch = familyToken && scenarioToken.includes(familyToken);
                const score = (exact ? 100 : 0) + (familyMatch ? 20 : 0) + (tokenMatch ? 50 : 0) + (familyScenarioMatch ? 40 : 0);
                return { scenario, score };
            })
            .filter((entry) => entry.score > 0)
            .sort((left, right) => right.score - left.score);
        if (scored.length) {
            const match = scored[0].scenario;
            return { id: match.id, title: match.title || match.id, color: performancePromiseColor(match.id) };
        }
        return { id: "unmapped", title: "Unmapped", color: performancePromiseColor("unmapped") };
    }

    function selectedPerformancePromise() {
        const scenarios = state.performanceReview?.scenarios || [];
        if (!scenarios.length) return null;
        return scenarios.find((scenario) => scenario.id === state.selectedPerformanceScenarioId) || scenarios[0];
    }

    function rowBelongsToPerformancePromise(item, scenario = selectedPerformancePromise()) {
        if (!scenario) return true;
        return Object.values(scenario.evidence || {})
            .some((rows) => Array.isArray(rows) && rows.some((row) => promiseEvidenceMatches(item, row)));
    }

    function promiseOverviewRow(scenario) {
        const status = scenarioStatus(scenario);
        const scale = bestScaleCheck(scenario);
        const observed = scale ? Number(scale.observed || 0) : 0;
        const target = scale ? Number(scale.target || 0) : 0;
        const scaleRatio = target > 0 ? observed / target : 0;
        return {
            id: scenario.id,
            title: scenario.title || scenario.id || "Scenario",
            promise: scenario.promise || "",
            status,
            scale,
            scaleRatio,
            budgetMisses: Number(scenario.budget_misses || 0),
            ceilingHits: Number(scenario.ceilings_reached || 0),
            maxLatencyMs: Number(scenario.max_latency_ms || 0),
            peakWorkingSet: Number(scenario.peak_working_set_bytes || 0),
            families: scenarioFamilies(scenario),
        };
    }

    function renderPromiseStatusGraph(rows) {
        const buckets = [
            { cls: "ok", label: "Met", rows: rows.filter((row) => row.status.cls === "ok") },
            { cls: "watch", label: "Watch", rows: rows.filter((row) => row.status.cls === "watch") },
            { cls: "bad", label: "Below target", rows: rows.filter((row) => row.status.cls === "bad") },
            { cls: "stale", label: "Untested", rows: rows.filter((row) => row.status.cls === "stale") },
        ];
        const total = Math.max(1, rows.length);
        return `<section class="panel-card promise-graph-card promise-graph-card--status">
            <div class="promise-graph-card__header">
                <h3>Promise Status</h3>
                <p>${formatNumber.format(buckets[0].rows.length)}/${formatNumber.format(rows.length)} met, ${formatNumber.format(buckets[1].rows.length + buckets[2].rows.length)} need attention.</p>
            </div>
            <div class="promise-status-track" aria-label="Promise status distribution">
                ${buckets.map((bucket) => bucket.rows.length ? `<span class="promise-status-track__${bucket.cls}" style="width:${(bucket.rows.length / total) * 100}%"></span>` : "").join("")}
            </div>
            <div class="promise-status-grid">
                ${buckets.map((bucket) => `<div class="promise-status-cell promise-status-cell--${bucket.cls}">
                    <span>${escapeHtml(bucket.label)}</span>
                    <strong>${formatNumber.format(bucket.rows.length)}</strong>
                </div>`).join("")}
            </div>
        </section>`;
    }

    function renderPromiseMetricGraph({ title, caption, rows, value, valueLabel, tone, maxValue, targetValue }) {
        const measuredRows = rows.map((row) => ({ row, value: Number(value(row) || 0) }));
        const max = Math.max(maxValue || 0, ...measuredRows.map((item) => item.value), 1);
        return `<section class="panel-card promise-graph-card">
            <div class="promise-graph-card__header">
                <h3>${escapeHtml(title)}</h3>
                <p>${escapeHtml(caption)}</p>
            </div>
            <div class="promise-graph-bars">
                ${measuredRows.map(({ row, value: rawValue }) => {
            const pct = Math.max(rawValue ? 4 : 0, Math.min(100, (rawValue / max) * 100));
            const marker = targetValue ? `<b style="left:${Math.min(100, (targetValue / max) * 100)}%"></b>` : "";
            const cls = tone(row);
            return `<button type="button" class="promise-graph-row promise-graph-row--${cls}" data-promise-focus="${escapeHtml(row.id)}" title="${escapeHtml(row.title)}" style="--promise-color:${escapeHtml(performancePromiseColor(row.id))}">
                    <span><strong>${escapeHtml(row.title)}</strong><em>${escapeHtml(row.status.label)}</em></span>
                    <div><i style="width:${pct}%"></i>${marker}</div>
                    <strong>${escapeHtml(valueLabel(row))}</strong>
                </button>`;
        }).join("")}
            </div>
        </section>`;
    }

    function renderPerformanceFocusList(rows) {
        const target = byId("performance-focus-list");
        if (!target) return;
        if (!rows.length) {
            target.innerHTML = `<p class="muted">No pressure/growth pairs loaded.</p>`;
            return;
        }
        const sorted = [...rows]
            .sort((left, right) => Number(right.score || 0) - Number(left.score || 0))
            .slice(0, 15);
        target.innerHTML = sorted.map((row, index) => {
            const cls = row.x >= 1 && row.y >= 1 ? "bad" : row.x >= 1 || row.y >= 1 ? "watch" : "ok";
            return `<div class="performance-focus-row" style="--promise-color:${escapeHtml(row.color || performancePromiseColor(row.promiseId || row.label))}" aria-label="${escapeHtml(row.label)} risk register row">
                <span class="rank-pill">${index + 1}</span>
                <span class="performance-focus-row__main">
                    <strong>${escapeHtml(row.label)}</strong>
                    <em>${escapeHtml(`${row.promiseTitle || "Unmapped"} - ${formatRatio(row.x)} load - ${formatRatio(row.y)} latency/unit`)}</em>
                </span>
                <span class="status-pill status-pill--${cls}">${escapeHtml(formatRatio(row.score || 0))}</span>
            </div>`;
        }).join("");
    }

    function promiseFocusScore(row) {
        const statusWeight = { bad: 600, stale: 450, watch: 300, ok: 0 }[row.status.cls] || 0;
        const scaleGap = row.scale && row.scale.met === false ? 250 : 0;
        const latencyWeight = row.maxLatencyMs ? Math.min(180, row.maxLatencyMs / 4) : 0;
        const memoryWeight = row.peakWorkingSet ? Math.min(120, row.peakWorkingSet / 32_000_000) : 0;
        return statusWeight + scaleGap + row.budgetMisses * 40 + row.ceilingHits * 45 + latencyWeight + memoryWeight;
    }

    function budgetLabelParts(item) {
        const raw = item.scenario_label || item.benchmark_key || item.name || "Scenario";
        const parameter = item.parameter_label || item.parameter_value || "";
        const details = [
            item.sourceLabel,
            parameter ? String(parameter) : "",
            titleCaseMetricName(item.workload_family || item.family || ""),
        ].filter(Boolean);
        return {
            title: titleCaseMetricName(String(raw).replace(/[_/]+/g, " ")).slice(0, 58),
            subtitle: [...new Set(details)].join(" · "),
        };
    }

    function buildCombinedBudgetPressureCard(rows) {
        const ratios = rows
            .filter((item) => item.threshold_ms)
            .map((item) => ({ item, ratio: budgetRatio(item) }))
            .filter((entry) => Number.isFinite(entry.ratio) && entry.ratio > 0)
            .sort((a, b) => b.ratio - a.ratio);
        if (!ratios.length) return emptyChartCard("Budget Pressure", "No budget thresholds available.");
        const visible = ratios.filter((entry) => entry.ratio >= 0.4).slice(0, 14);
        const rowsToRender = visible.length ? visible : ratios.slice(0, 14);
        const maxRatio = Math.max(2, ...rowsToRender.map((entry) => entry.ratio));
        const rowsMarkup = rowsToRender.map(({ item, ratio }) => {
            const parts = budgetLabelParts(item);
            const pct = Math.min(100, (ratio / maxRatio) * 100);
            const cls = ratio > 1 ? "bad" : ratio > 0.85 ? "watch" : "ok";
            return `<div class="headroom-row">
                <span><strong>${escapeHtml(parts.title)}</strong><em>${escapeHtml(parts.subtitle)}</em></span>
                <div><i class="headroom-row__fill headroom-row__fill--${cls}" style="width:${pct}%"></i><b style="left:${(1 / maxRatio) * 100}%"></b></div>
                <strong>${formatRatio(ratio)}</strong>
            </div>`;
        }).join("");
        const over = ratios.filter((entry) => entry.ratio > 1).length;
        const watch = ratios.filter((entry) => entry.ratio >= 0.85 && entry.ratio <= 1).length;
        return `<section class="panel-card chart-panel" id="budget-pressure">
            <div><h3>Budget Pressure</h3><p class="chart-caption">${formatNumber.format(over)} over budget · ${formatNumber.format(watch)} near budget · full rows stay in the dataset tables.</p></div>
            <figure class="chart-frame chart-frame--list" aria-label="Budget pressure">${rowsMarkup}</figure>
        </section>`;
    }

    function buildCombinedLatencyDistributionCard(rows) {
        const items = rows
            .filter((item) => item.threshold_ms && item.median_ns)
            .map((item) => ({
                item,
                mean: latencyMs(item),
                median: Number(item.median_ns || 0) / 1_000_000,
                dispersion: Number(item.dispersion_ns || 0) / 1_000_000,
                budget: Number(item.threshold_ms || 0),
            }))
            .sort((a, b) => (b.mean / b.budget) - (a.mean / a.budget))
            .slice(0, 14);
        if (!items.length) return emptyChartCard("Latency Spread", "No median and dispersion rows available.");
        const max = Math.max(...items.flatMap((item) => [item.mean + item.dispersion, item.budget]), 1);
        const rowsMarkup = items.map((row) => {
            const parts = budgetLabelParts(row.item);
            const start = Math.max(0, row.median - row.dispersion);
            const end = row.median + row.dispersion;
            return `<div class="distribution-row">
                <span><strong>${escapeHtml(parts.title)}</strong><em>${escapeHtml(parts.subtitle)}</em></span>
                <div>
                    <i style="left:${(start / max) * 100}%;width:${Math.max(1, ((end - start) / max) * 100)}%"></i>
                    <b class="distribution-row__median" style="left:${(row.median / max) * 100}%"></b>
                    <b class="distribution-row__mean" style="left:${(row.mean / max) * 100}%"></b>
                    <b class="distribution-row__budget" style="left:${(row.budget / max) * 100}%"></b>
                </div>
                <strong>${formatMs(row.mean)}</strong>
            </div>`;
        }).join("");
        return `<section class="panel-card chart-panel" id="latency-spread">
            <div><h3>Latency Spread</h3><p class="chart-caption">Search and editor distributions share one view. Whisker is median ± dispersion, dot is mean, guide is budget.</p></div>
            <figure class="chart-frame chart-frame--list" aria-label="Latency spread">${rowsMarkup}</figure>
        </section>`;
    }

    function buildCapacityResourceCard(capacityRows, resourceRows) {
        const capacityMarkup = capacityRows.slice(0, 6).map((item) => {
            const samples = item.samples || [];
            const maxValue = Math.max(...samples.map((sample) => Number(sample.workload_value || 0)), Number(item.first_failure_workload || 0), 1);
            const rungs = samples.map((sample) => {
                const failed = item.first_failure_workload != null && Number(sample.workload_value || 0) >= Number(item.first_failure_workload || Infinity);
                return `<i class="capacity-ladder__rung capacity-ladder__rung--${failed ? "bad" : "ok"}" style="left:${Math.min(100, (Number(sample.workload_value || 0) / maxValue) * 100)}%" title="${escapeHtml(sample.workload_label || "")}"></i>`;
            }).join("");
            return `<div class="capacity-ladder__row">
                <span><strong>${escapeHtml(item.scenario_label || item.scenario)}</strong><em>${escapeHtml(item.failure_mode || "not_reached")}</em></span>
                <div>${rungs}</div>
                <strong>${escapeHtml(item.first_failure_label || `>${item.last_successful_label || "-"}`)}</strong>
            </div>`;
        }).join("");
        const resourceMarkup = performanceResultBars(topResourceRows(resourceRows, 5), "resource");
        return `<section class="panel-card chart-panel chart-panel--wide" id="capacity-memory">
            <div><h3>Capacity &amp; Memory</h3><p class="chart-caption">Capacity ladders and peak live memory are paired because they explain the same scaling risk from two angles.</p></div>
            <div class="capacity-resource-grid">
                <figure class="chart-frame chart-frame--list" aria-label="Capacity ladder">${capacityMarkup || `<p class="muted">No capacity samples loaded.</p>`}</figure>
                <figure class="chart-frame chart-frame--list" aria-label="Peak resource use">${resourceMarkup}</figure>
            </div>
        </section>`;
    }

    function emptyChartCard(title, message) {
        return `<section class="panel-card chart-panel"><div><h3>${escapeHtml(title)}</h3></div><div class="chart-empty">${escapeHtml(message)}</div></section>`;
    }

    function buildSearchSpeedSeries(items, predicate, keyFn, describeFn) {
        const groups = new Map();

        items.filter(predicate).forEach((item) => {
            const key = keyFn(item);
            if (!groups.has(key)) {
                groups.set(key, []);
            }
            groups.get(key).push(item);
        });

        return Array.from(groups.entries())
            .map(([key, group]) => {
                const ordered = [...group].sort((left, right) => (left.parameter_value ?? 0) - (right.parameter_value ?? 0));
                return {
                    key,
                    ...describeFn(key, ordered[0]),
                    points: ordered.map((item) => ({
                        xValue: item.parameter_value ?? 0,
                        xLabel: item.parameter_label || String(item.parameter_value ?? "-"),
                        meanMs: item.mean_ns / 1_000_000,
                        thresholdMs: item.threshold_ms,
                        throughput: item.throughput_mb_s || 0,
                    })),
                };
            })
            .sort((left, right) => (left.order ?? 0) - (right.order ?? 0));
    }

    function buildSearchSpeedLineCard({ title, subtitle, series, insights, hardLimitText, controls = "" }) {
        if (!series.length) {
            return `<section class="panel-card chart-panel">
                <div class="chart-panel__header"><div><h3>${escapeHtml(title)}</h3><p class="chart-caption">${escapeHtml(subtitle)}</p></div>${controls}</div>
                <div class="chart-empty">No matching records for this chart.</div>
            </section>`;
        }
        series = withDistinctSeriesColors(series);

        const orderedX = Array.from(
            new Map(
                series
                    .flatMap((entry) => entry.points)
                    .sort((left, right) => left.xValue - right.xValue)
                    .map((point) => [point.xValue, point.xLabel])
            ).entries()
        );

        const allValues = series.flatMap((entry) => entry.points.map((point) => Math.max(point.meanMs, 0.001)));
        let minValue = Math.min(...allValues);
        let maxValue = Math.max(...allValues);
        if (minValue === maxValue) {
            minValue *= 0.5;
            maxValue *= 1.5;
        }
        const yTicks = buildLogTicks(minValue, maxValue);
        const yMin = yTicks[0];
        const yMax = yTicks[yTicks.length - 1];

        const width = 760;
        const height = 320;
        const left = 64;
        const right = 24;
        const top = 24;
        const bottom = 52;
        const plotWidth = width - left - right;
        const plotHeight = height - top - bottom;
        const xStep = orderedX.length > 1 ? plotWidth / (orderedX.length - 1) : 0;
        const xLookup = new Map(
            orderedX.map(([value], index) => [value, orderedX.length === 1 ? left + plotWidth / 2 : left + index * xStep])
        );
        const logMin = Math.log10(yMin);
        const logMax = Math.log10(yMax);
        const yPosition = (value) => {
            const safeValue = Math.max(value, yMin);
            const ratio = (Math.log10(safeValue) - logMin) / Math.max(logMax - logMin, 0.0001);
            return top + plotHeight - ratio * plotHeight;
        };

        const gridLines = yTicks.map((tick) => {
            const y = yPosition(tick);
            return `<g>
                <line class="chart-grid-line" x1="${left}" y1="${y}" x2="${width - right}" y2="${y}"></line>
                <text class="chart-tick-label" x="${left - 10}" y="${y + 4}" text-anchor="end">${escapeHtml(formatAxisMs(tick))}</text>
            </g>`;
        }).join("");

        const xTicks = orderedX.map(([value, label]) => {
            const x = xLookup.get(value);
            return `<g>
                <line class="chart-axis-line" x1="${x}" y1="${height - bottom}" x2="${x}" y2="${height - bottom + 6}"></line>
                <text class="chart-tick-label" x="${x}" y="${height - bottom + 22}" text-anchor="middle">${escapeHtml(label)}</text>
            </g>`;
        }).join("");

        const seriesMarkup = series.map((entry) => {
            const path = entry.points
                .map((point, index) => `${index === 0 ? "M" : "L"} ${xLookup.get(point.xValue)} ${yPosition(point.meanMs)}`)
                .join(" ");
            const markers = entry.points.map((point) => {
                const x = xLookup.get(point.xValue);
                const y = yPosition(point.meanMs);
                const overBudget = point.meanMs > point.thresholdMs;
                return `<g>
                    <circle class="chart-point" cx="${x}" cy="${y}" r="5" stroke="${overBudget ? "#ff7474" : entry.color}" fill="#10151c"></circle>
                    ${overBudget ? `<circle class="chart-point--over" cx="${x}" cy="${y}" r="9"></circle>` : ""}
                </g>`;
            }).join("");
            return `<g>
                <path class="chart-series-line" d="${path}" stroke="${entry.color}"${entry.dasharray ? ` stroke-dasharray="${entry.dasharray}"` : ""}></path>
                ${markers}
            </g>`;
        }).join("");

        const legend = renderChartLegend(series);
        const insightMarkup = [...insights, hardLimitText]
            .filter(Boolean)
            .map((item) => `<li>${escapeHtml(item)}</li>`)
            .join("");

        return `<section class="panel-card chart-panel">
            <div class="chart-panel__header">
                <div>
                    <h3>${escapeHtml(title)}</h3>
                    <p class="chart-caption">${escapeHtml(subtitle)}</p>
                </div>
                ${controls}
            </div>
            <div class="chart-frame">
                <svg class="chart-svg" viewBox="0 0 ${width} ${height}" role="img" aria-label="${escapeHtml(title)}">
                    ${gridLines}
                    <line class="chart-axis-line" x1="${left}" y1="${top}" x2="${left}" y2="${height - bottom}"></line>
                    <line class="chart-axis-line" x1="${left}" y1="${height - bottom}" x2="${width - right}" y2="${height - bottom}"></line>
                    ${xTicks}
                    ${seriesMarkup}
                    <text class="chart-axis-label" x="${width / 2}" y="${height - 12}" text-anchor="middle">Scale</text>
                    <text class="chart-axis-label" x="18" y="${top + plotHeight / 2}" text-anchor="middle" transform="rotate(-90 18 ${top + plotHeight / 2})">Time (ms, log scale)</text>
                </svg>
            </div>
            <div class="chart-legend">${legend}</div>
            <ul class="chart-insights">${insightMarkup}</ul>
        </section>`;
    }

    function buildSearchDependencyCard(metrics) {
        if (!metrics.length) {
            return `<section class="panel-card chart-panel"><div><h3>Relative Dependency</h3><p class="chart-caption">Time multiplier when each growth axis doubles.</p></div><div class="chart-empty">No matching records for dependency analysis.</div></section>`;
        }

        const width = 760;
        const height = 320;
        const left = 60;
        const right = 24;
        const top = 24;
        const bottom = 52;
        const plotWidth = width - left - right;
        const plotHeight = height - top - bottom;
        const barValues = metrics.flatMap((item) => [item.completionMultiplier, item.firstResponseMultiplier].filter((value) => Number.isFinite(value)));
        const maxValue = Math.max(...barValues, 1);
        const yMax = Math.max(1.2, Math.ceil(maxValue * 1.2 * 10) / 10);
        const yTicks = buildLinearTicks(yMax);
        const groupWidth = plotWidth / metrics.length;
        const barWidth = Math.min(46, groupWidth * 0.28);
        const gap = Math.min(18, groupWidth * 0.08);
        const yPosition = (value) => top + plotHeight - (value / yMax) * plotHeight;

        const gridLines = yTicks.map((tick) => {
            const y = yPosition(tick);
            return `<g>
                <line class="chart-grid-line" x1="${left}" y1="${y}" x2="${width - right}" y2="${y}"></line>
                <text class="chart-tick-label" x="${left - 10}" y="${y + 4}" text-anchor="end">${escapeHtml(formatNumber.format(tick))}x</text>
            </g>`;
        }).join("");

        const bars = metrics.map((metric, index) => {
            const groupCenter = left + groupWidth * index + groupWidth / 2;
            const completionX = groupCenter - barWidth - gap / 2;
            const responseX = groupCenter + gap / 2;
            const completionHeight = (metric.completionMultiplier / yMax) * plotHeight;
            const responseHeight = (metric.firstResponseMultiplier / yMax) * plotHeight;
            const completionY = yPosition(metric.completionMultiplier);
            const responseY = yPosition(metric.firstResponseMultiplier);
            return `<g>
                <rect class="chart-bar--completion" x="${completionX}" y="${completionY}" width="${barWidth}" height="${completionHeight}" rx="8"></rect>
                <rect class="chart-bar--first-response" x="${responseX}" y="${responseY}" width="${barWidth}" height="${responseHeight}" rx="8"></rect>
                <text class="chart-value-label" x="${completionX + barWidth / 2}" y="${completionY - 8}" text-anchor="middle">${escapeHtml(formatNumber.format(metric.completionMultiplier))}x</text>
                <text class="chart-value-label" x="${responseX + barWidth / 2}" y="${responseY - 8}" text-anchor="middle">${escapeHtml(formatNumber.format(metric.firstResponseMultiplier))}x</text>
                <text class="chart-tick-label" x="${groupCenter}" y="${height - bottom + 22}" text-anchor="middle">${escapeHtml(metric.label)}</text>
            </g>`;
        }).join("");

        const legend = [
            { label: "Completion", color: searchLatencyColors.completion },
            { label: "First response", color: searchLatencyColors.first_response },
        ].map((item) => `<span class="chart-legend__item">
                <svg class="chart-legend__swatch" viewBox="0 0 28 10" aria-hidden="true"><line x1="1" y1="5" x2="27" y2="5" stroke="${item.color}" stroke-width="5"></line></svg>
                <span>${escapeHtml(item.label)}</span>
            </span>`).join("");

        const insights = buildDependencyInsights(metrics)
            .map((item) => `<li>${escapeHtml(item)}</li>`)
            .join("");

        return `<section class="panel-card chart-panel">
            <div>
                <h3>Relative Dependency</h3>
                <p class="chart-caption">Time multiplier when each growth axis doubles. 1.0x is flat, 2.0x is linear, above 2.0x means time degrades faster than the input grows.</p>
            </div>
            <div class="chart-frame">
                <svg class="chart-svg" viewBox="0 0 ${width} ${height}" role="img" aria-label="Relative dependency of search speed on tabs, files, and file size">
                    ${gridLines}
                    <line class="chart-axis-line" x1="${left}" y1="${top}" x2="${left}" y2="${height - bottom}"></line>
                    <line class="chart-axis-line" x1="${left}" y1="${height - bottom}" x2="${width - right}" y2="${height - bottom}"></line>
                    ${bars}
                    <text class="chart-axis-label" x="${width / 2}" y="${height - 12}" text-anchor="middle">Growth axis</text>
                    <text class="chart-axis-label" x="18" y="${top + plotHeight / 2}" text-anchor="middle" transform="rotate(-90 18 ${top + plotHeight / 2})">Time multiplier per 2x growth</text>
                </svg>
            </div>
            <div class="chart-legend">${legend}</div>
            <ul class="chart-insights">${insights}</ul>
        </section>`;
    }

    function buildAggregateScopeInsights(series) {
        const completion = series.find((item) => item.latencyKind === "completion");
        const firstResponse = series.find((item) => item.latencyKind === "first_response");
        const insights = [];

        if (completion) {
            const overBudget = completion.points.find((point) => point.meanMs > point.thresholdMs);
            insights.push(
                overBudget
                    ? `Completion crosses its budget at ${overBudget.xLabel}.`
                    : `Completion stays within budget through ${completion.points[completion.points.length - 1].xLabel}.`
            );
        }

        if (firstResponse) {
            const overBudget = firstResponse.points.find((point) => point.meanMs > point.thresholdMs);
            insights.push(
                overBudget
                    ? `First response crosses its budget at ${overBudget.xLabel}.`
                    : `First response stays within budget through ${firstResponse.points[firstResponse.points.length - 1].xLabel}.`
            );
        }

        const completionMultiplier = completion ? calculateDoublingMultiplier(completion.points) : null;
        const responseMultiplier = firstResponse ? calculateDoublingMultiplier(firstResponse.points) : null;
        if (Number.isFinite(completionMultiplier) && Number.isFinite(responseMultiplier)) {
            insights.push(
                `Completion is ${describeGrowth(completionMultiplier)} while first response is ${describeGrowth(responseMultiplier)}.`
            );
        }

        return insights;
    }

    function buildFileSizeInsights(series) {
        const completionSeries = series.filter((item) => item.latencyKind === "completion");
        const firstResponseSeries = series.filter((item) => item.latencyKind === "first_response");
        const insights = [];

        const completionBreaks = completionSeries
            .map((item) => {
                const overBudget = item.points.find((point) => point.meanMs > point.thresholdMs);
                return overBudget ? `${item.shortLabel} at ${overBudget.xLabel}` : null;
            })
            .filter(Boolean);
        insights.push(
            completionBreaks.length
                ? `Completion budget breaks start at ${completionBreaks.join("; ")}.`
                : "Completion stays within budget across all measured file-size series."
        );

        const firstResponseBreaks = firstResponseSeries
            .map((item) => {
                const overBudget = item.points.find((point) => point.meanMs > point.thresholdMs);
                return overBudget ? `${item.shortLabel} at ${overBudget.xLabel}` : null;
            })
            .filter(Boolean);
        insights.push(
            firstResponseBreaks.length
                ? `First response budget breaks start at ${firstResponseBreaks.join("; ")}.`
                : "First response stays within budget across Active, Current, and All."
        );

        const completionMultiplier = mean(
            completionSeries
                .map((item) => calculateDoublingMultiplier(item.points))
                .filter((value) => Number.isFinite(value))
        );
        const responseMultiplier = mean(
            firstResponseSeries
                .map((item) => calculateDoublingMultiplier(item.points))
                .filter((value) => Number.isFinite(value))
        );
        if (Number.isFinite(completionMultiplier) && Number.isFinite(responseMultiplier)) {
            insights.push(
                `Across modes, completion is ${describeGrowth(completionMultiplier)} while first response is ${describeGrowth(responseMultiplier)}.`
            );
        }

        return insights;
    }

    function buildSearchDependencyMetrics(items) {
        const dimensions = [
            {
                label: "Tabs",
                completionSeries: buildSearchSpeedSeries(
                    items,
                    (item) => item.mode === "all" && item.scaling_axis === "aggregate_size" && item.latency_kind === "completion",
                    () => "completion",
                    () => ({ order: 0 })
                ),
                firstResponseSeries: buildSearchSpeedSeries(
                    items,
                    (item) => item.mode === "all" && item.scaling_axis === "aggregate_size" && item.latency_kind === "first_response",
                    () => "first_response",
                    () => ({ order: 0 })
                ),
            },
            {
                label: "Files",
                completionSeries: buildSearchSpeedSeries(
                    items,
                    (item) => item.mode === "current" && item.scaling_axis === "aggregate_size" && item.latency_kind === "completion",
                    () => "completion",
                    () => ({ order: 0 })
                ),
                firstResponseSeries: buildSearchSpeedSeries(
                    items,
                    (item) => item.mode === "current" && item.scaling_axis === "aggregate_size" && item.latency_kind === "first_response",
                    () => "first_response",
                    () => ({ order: 0 })
                ),
            },
            {
                label: "File size",
                completionSeries: buildSearchSpeedSeries(
                    items,
                    (item) => item.scaling_axis === "file_size" && item.latency_kind === "completion",
                    (item) => item.mode,
                    (mode) => ({ order: { active: 0, current: 1, all: 2 }[mode] ?? 9 })
                ),
                firstResponseSeries: buildSearchSpeedSeries(
                    items,
                    (item) => item.scaling_axis === "file_size" && item.latency_kind === "first_response",
                    (item) => item.mode,
                    (mode) => ({ order: { active: 0, current: 1, all: 2 }[mode] ?? 9 })
                ),
            },
        ];

        return dimensions.map((dimension) => ({
            label: dimension.label,
            completionMultiplier: mean(
                dimension.completionSeries
                    .map((entry) => calculateDoublingMultiplier(entry.points))
                    .filter((value) => Number.isFinite(value))
            ),
            firstResponseMultiplier: mean(
                dimension.firstResponseSeries
                    .map((entry) => calculateDoublingMultiplier(entry.points))
                    .filter((value) => Number.isFinite(value))
            ),
        })).filter((item) => Number.isFinite(item.completionMultiplier) && Number.isFinite(item.firstResponseMultiplier));
    }

    function buildDependencyInsights(metrics) {
        const completionSorted = [...metrics].sort((left, right) => right.completionMultiplier - left.completionMultiplier);
        const responseSorted = [...metrics].sort((left, right) => right.firstResponseMultiplier - left.firstResponseMultiplier);
        const flattestResponse = [...metrics].sort((left, right) => left.firstResponseMultiplier - right.firstResponseMultiplier)[0];
        const insights = [];

        if (completionSorted[0]) {
            insights.push(
                `Completion depends most on ${completionSorted[0].label.toLowerCase()} growth at ${formatNumber.format(completionSorted[0].completionMultiplier)}x time per 2x scale.`
            );
        }
        if (responseSorted[0]) {
            insights.push(
                `First response depends most on ${responseSorted[0].label.toLowerCase()} growth at ${formatNumber.format(responseSorted[0].firstResponseMultiplier)}x time per 2x scale.`
            );
        }
        if (flattestResponse) {
            insights.push(
                `First response is flattest against ${flattestResponse.label.toLowerCase()}, which is consistent with the capped-result benchmark path.`
            );
        }

        return insights;
    }

    function renderSpeedTriageVisual(triage, triageSummary = {}) {
        if (!triage.length) {
            return '<p class="muted">No coordinated triage data loaded.</p>';
        }
        const groups = new Map();
        triage.forEach((item) => {
            const key = [
                item.scenario_label || item.scenario_id || "Unknown scenario",
                item.family || "unmapped",
                item.suspected_limiting_resource || "resource",
                item.recommended_action || "",
            ].join("|");
            const group = groups.get(key) || {
                label: item.scenario_label || item.scenario_id || "Unknown scenario",
                family: item.family || "unmapped",
                resource: item.suspected_limiting_resource || "resource",
                action: item.recommended_action || "",
                count: 0,
                rankScore: 0,
                reasons: new Set(),
            };
            group.count += 1;
            group.rankScore = Math.max(group.rankScore, Number(item.rank_score || 0));
            pillValues(item.reason).forEach((reason) => group.reasons.add(reason));
            groups.set(key, group);
        });
        const cards = [...groups.values()]
            .sort((a, b) => b.rankScore - a.rankScore)
            .slice(0, 5);
        const maxScore = cards[0]?.rankScore || 1;
        const total = (triageSummary.critical || 0) + (triageSummary.watch || 0) + (triageSummary.ok || 0);
        const criticalPct = total ? ((triageSummary.critical || 0) / total) * 100 : 0;
        const watchPct = total ? ((triageSummary.watch || 0) / total) * 100 : 0;
        const okPct = Math.max(0, 100 - criticalPct - watchPct);
        return `<div class="triage-snapshot">
            <div class="triage-severity" aria-label="Triage severity distribution">
                <div class="triage-severity__track">
                    <span class="triage-severity__critical" style="width:${criticalPct}%"></span>
                    <span class="triage-severity__watch" style="width:${watchPct}%"></span>
                    <span class="triage-severity__ok" style="width:${okPct}%"></span>
                </div>
                <div class="triage-severity__legend">
                    <span><strong>${formatNumber.format(triageSummary.critical || 0)}</strong> critical</span>
                    <span><strong>${formatNumber.format(triageSummary.watch || 0)}</strong> watch</span>
                    <span><strong>${formatNumber.format(triageSummary.ok || 0)}</strong> ok</span>
                </div>
            </div>
            <div class="triage-visual">
                ${cards.map((item, index) => renderSpeedTriageCard(item, index, maxScore)).join("")}
            </div>
        </div>`;
    }

    function renderSpeedTriageCard(item, index, maxScore) {
        const scorePct = Math.max(6, Math.min(100, (item.rankScore / maxScore) * 100));
        const reasons = [...item.reasons]
            .filter((reason) => /over budget|slow\s*>|ceiling|failure|near/i.test(reason))
            .slice(0, 3);
        return `<article class="triage-card">
            <div class="triage-card__rank">${index + 1}</div>
            <div class="triage-card__body">
                <div class="triage-card__header" title="Rank score ${escapeHtml(formatNumber.format(item.rankScore))}">
                    <h3>${escapeHtml(item.label)}</h3>
                </div>
                <div class="triage-card__bar"><span style="width:${scorePct}%"></span></div>
                <div class="triage-card__chips">
                    <span class="pill">${escapeHtml(item.family)}</span>
                    <span class="pill">${escapeHtml(item.resource)}</span>
                </div>
                ${reasons.length ? `<div class="triage-card__signals">
                    ${reasons.map((reason) => `<span>${escapeHtml(reason)}</span>`).join("")}
                </div>` : ""}
            </div>
        </article>`;
    }

    function renderResourceSampleDetails(samples) {
        if (!samples.length) {
            return '<span class="muted">No samples</span>';
        }
        const rows = samples.map((sample) => `<tr>
            <td>${escapeHtml(sample.workload_label || "-")}</td>
            <td>${formatNumber.format(sample.elapsed_ms || 0)} ms</td>
            <td>${escapeHtml(formatBytes(sample.allocated_bytes))}<div class="muted">${formatNumber.format(sample.allocation_count || 0)} allocs / ${formatNumber.format(sample.reallocation_count || 0)} reallocs</div></td>
            <td>${escapeHtml(formatBytes(sample.peak_live_bytes))}</td>
            <td>${escapeHtml(formatBytes(sample.working_set_bytes))}</td>
            <td>${escapeHtml(formatBytes(sample.manifest_size_bytes))}</td>
            <td>${sample.page_fault_count == null ? "-" : formatNumber.format(sample.page_fault_count)}</td>
            <td>${sample.handle_count == null ? "-" : formatNumber.format(sample.handle_count)}</td>
            <td>${escapeHtml(sample.result_label || "-")}</td>
            <td class="${sample.status === "ok" ? "risk-good" : "risk-bad"}">${escapeHtml(sample.status || "-")}${sample.note ? `<div class="muted">${escapeHtml(sample.note)}</div>` : ""}</td>
        </tr>`).join("");
        return `<details class="inline-samples">
            <summary>${formatNumber.format(samples.length)} samples</summary>
            <div class="inline-samples__table">
                <table>
                    <thead><tr><th>Workload</th><th>Elapsed</th><th>Allocated</th><th>Peak live</th><th>Working set</th><th>Manifest</th><th>Page faults</th><th>Handles</th><th>Result</th><th>Status</th></tr></thead>
                    <tbody>${rows}</tbody>
                </table>
            </div>
        </details>`;
    }

    function renderPerformanceReviewCoverage() {
        const payload = state.performanceReview || {};
        const scenarios = payload.scenarios || [];

        renderPerformanceStaleState(payload);
        renderPerformancePromiseBoard(scenarios);
        renderPerformancePromiseDetail(selectedPerformancePromise());
    }

    function renderPerformanceStaleState(payload) {
        const target = byId("performance-stale-state");
        if (!target) return;
        const sources = payload?.meta?.source_artifacts || [];
        const missing = sources.filter((item) => !item.available || item.status === "failed");
        if (!missing.length) {
            target.innerHTML = "";
            return;
        }
        target.innerHTML = `<div class="performance-stale-state__banner performance-stale-state__banner--warn">
            <strong>Performance verdict is partial.</strong>
            <span>${escapeHtml(missing.map((item) => `${item.id}: ${item.error || item.status || "missing"}`).join(" • "))}</span>
        </div>`;
    }

    function renderPerformancePromiseBoard(scenarios) {
        const target = byId("performance-promise-board");
        if (!target) return;
        if (!scenarios.length) {
            target.innerHTML = `<p class="muted">No performance promise artifact loaded.</p>`;
            return;
        }
        if (!scenarios.some((scenario) => scenario.id === state.selectedPerformanceScenarioId)) {
            state.selectedPerformanceScenarioId = scenarios[0].id;
        }
        const selected = scenarios.find((scenario) => scenario.id === state.selectedPerformanceScenarioId) || scenarios[0];
        target.innerHTML = `<div class="promise-tabs" role="tablist" aria-label="Performance promises">
            ${scenarios.map((scenario) => {
            const status = scenarioStatus(scenario);
            const missCount = Number(scenario.budget_misses || 0);
            const ceilingCount = Number(scenario.ceilings_reached || 0);
            const active = scenario.id === selected.id;
            const color = performancePromiseColor(scenario.id);
            const pressure = [
                missCount ? `${formatNumber.format(missCount)} over` : "",
                ceilingCount ? `${formatNumber.format(ceilingCount)} ceilings` : "",
            ].filter(Boolean).join(" - ");
            return `<button type="button" class="promise-tab promise-tab--${status.cls} ${active ? "is-active" : ""}" role="tab" aria-selected="${active ? "true" : "false"}" title="${escapeHtml(scenario.title || scenario.id || "Scenario")}" data-promise-tab="${escapeHtml(scenario.id)}" style="--promise-color:${escapeHtml(color)}">
                    <strong>${escapeHtml(scenario.title || scenario.id || "Scenario")}</strong>
                    <span>${escapeHtml(pressure || status.label)}</span>
                </button>`;
        }).join("")}
        </div>
        ${renderScenarioPromisePanel(selected)}`;
    }

    function renderScenarioPromisePanel(scenario) {
        return `<section class="promise-tab-panel promise-tab-panel--summary" role="tabpanel" aria-label="${escapeHtml(scenario.title || scenario.id || "Scenario")} promise">
            <p class="promise-tab-promise">${escapeHtml(scenario.promise || "No promise text loaded.")}</p>
        </section>`;
    }

    function renderScenarioProgressCells(scenario) {
        const status = scenarioStatus(scenario);
        const scale = bestScaleCheck(scenario);
        const observed = scale ? formatScaleValue(scale.observed, scale.unit) : "-";
        const targetValue = scale ? formatScaleValue(scale.target, scale.unit) : "-";
        const missCount = Number(scenario.budget_misses || 0);
        const ceilingCount = Number(scenario.ceilings_reached || 0);
        const progress = [
            ["Status", status.label, status.cls],
            ["Observed", observed, scale?.met === false ? "bad" : "ok"],
            ["Target", targetValue, "neutral"],
            ["Budget misses", formatNumber.format(missCount), missCount ? "bad" : "ok"],
            ["Ceilings hit", formatNumber.format(ceilingCount), ceilingCount ? "watch" : "ok"],
            ["Worst latency", scenario.max_latency_ms ? formatMs(scenario.max_latency_ms) : "-", missCount ? "watch" : "neutral"],
            ["Peak working set", scenario.peak_working_set_bytes ? formatBytes(scenario.peak_working_set_bytes) : "-", "neutral"],
        ];
        return `<div class="promise-tab-progress">
            ${progress.map(([label, value, cls]) => `<div class="promise-progress-cell promise-progress-cell--${cls}">
                <span>${escapeHtml(label)}</span>
                <strong>${escapeHtml(value)}</strong>
            </div>`).join("")}
        </div>`;
    }

    function renderScenarioScaleChecks(scenario) {
        const checks = scenario.scale_checks || [];
        if (!checks.length) return "";
        return `<section class="promise-subsection">
            <h3>Progress Toward Promise</h3>
            <div class="promise-scale-grid">
                ${checks.map((check) => `<div class="promise-scale-cell promise-scale-cell--${check.met ? "ok" : "bad"}">
                    <span>${escapeHtml(check.label || check.id || "Scale target")}</span>
                    <strong>${escapeHtml(formatScaleValue(check.observed, check.unit))}</strong>
                    <em>target ${escapeHtml(formatScaleValue(check.target, check.unit))}</em>
                </div>`).join("")}
            </div>
        </section>`;
    }

    function renderPerformancePromiseDetail(scenario) {
        const target = byId("performance-promise-detail");
        if (!target) return;
        if (!scenario) {
            target.innerHTML = `<p class="muted">No performance promise artifact loaded.</p>`;
            return;
        }
        const status = scenarioStatus(scenario);
        const filters = performanceBucketFilters(scenario.id);
        target.innerHTML = `<section class="performance-promise-detail-panel performance-promise-detail-panel--${status.cls}" style="--promise-color:${escapeHtml(performancePromiseColor(scenario.id))}" aria-label="${escapeHtml(scenario.title || scenario.id || "Scenario")} evidence">
            <div class="performance-promise-detail__header">
                <span class="performance-dataset-summary">
                    <i aria-hidden="true"></i>
                    <strong>${escapeHtml(scenario.title || scenario.id || "Scenario")}</strong>
                    <span class="status-pill status-pill--${escapeHtml(status.cls)}">${escapeHtml(status.label)}</span>
                </span>
                <span class="performance-dataset-pressure">${escapeHtml(formatScenarioPressure(scenario))}</span>
            </div>
            ${renderScenarioProgressCells(scenario)}
            ${renderScenarioScaleChecks(scenario)}
            ${renderScenarioEvidenceSections(scenario, { filters })}
        </section>`;
        renderScenarioFlamegraphs(scenario);
    }

    function performanceBucketFilters(scenarioId) {
        state.performanceBucketFilters[scenarioId] = {
            latency: "",
            capacity: "",
            resources: "",
            profiles: "",
            implementations: "",
            ...(state.performanceBucketFilters[scenarioId] || {}),
        };
        return state.performanceBucketFilters[scenarioId];
    }

    function formatScenarioPressure(scenario) {
        const missCount = Number(scenario.budget_misses || 0);
        const ceilingCount = Number(scenario.ceilings_reached || 0);
        return [
            missCount ? `${formatNumber.format(missCount)} over budget` : "0 over budget",
            ceilingCount ? `${formatNumber.format(ceilingCount)} ceilings` : "0 ceilings",
        ].join(" - ");
    }

    function renderScenarioEvidenceSections(scenario, options = {}) {
        const evidence = scenario.evidence || {};
        const filters = options.filters || {};
        const latencyRows = filterScenarioEvidenceRows(evidence.latency || [], filters.latency);
        const capacityRows = filterScenarioEvidenceRows(evidence.capacity || [], filters.capacity);
        const resourceRows = filterScenarioEvidenceRows(evidence.resources || [], filters.resources);
        const profileRows = filterScenarioEvidenceRows(evidence.profiles || [], filters.profiles);
        const implementationRows = filterScenarioEvidenceRows(scenario.implementations || [], filters.implementations);
        return `<div class="promise-evidence-grid">
            ${renderPerformanceSectionWithFilter(scenario.id, "capacity", filters.capacity, "Filter ceiling health checks...", renderScenarioCapacityEvidence(capacityRows, { scenario }))}
            ${renderPerformanceSectionWithFilter(scenario.id, "latency", filters.latency, "Filter targeted latency paths...", renderScenarioLatencyEvidence(latencyRows, { scenario }))}
            ${renderPerformanceSectionWithFilter(scenario.id, "resources", filters.resources, "Filter targeted resource paths...", renderScenarioResourceEvidence(resourceRows, { open: false }))}
            ${renderPerformanceSectionWithFilter(scenario.id, "profiles", filters.profiles, "Filter flamegraph profiles...", renderScenarioProfileEvidence(profileRows, { open: false, title: "Flamegraph Profiles", tail: renderScenarioFlamegraphBrowser(scenario, filters) }))}
            ${renderPerformanceSectionWithFilter(scenario.id, "implementations", filters.implementations, "Filter implementation audit...", renderScenarioImplementationEvidence(implementationRows, { open: false }))}
        </div>`;
    }

    function renderPerformanceSectionWithFilter(scenarioId, sectionId, value, placeholder, content) {
        return `<section class="performance-evidence-section">
            <div class="performance-section-toolbar">
                <input class="filter-input" type="search" data-performance-section-filter="${escapeHtml(sectionId)}" data-performance-scenario-id="${escapeHtml(scenarioId)}" value="${escapeHtml(value || "")}" placeholder="${escapeHtml(placeholder)}" />
            </div>
            ${content}
        </section>`;
    }

    function filterScenarioEvidenceRows(rows, query = "") {
        return (rows || []).filter((item) => {
            return matchesPerformanceDatasetSearch(item) && matchesFilter(item, query || "");
        });
    }

    function renderScenarioFlamegraphBrowser(scenario, filters = {}) {
        const profileRows = filterScenarioEvidenceRows(scenario.evidence?.profiles || [], filters.profiles);
        const profiles = profileRows;
        const available = profiles.filter((item) => item.available).length;
        return `<section class="performance-evidence-section performance-flamegraph-section">
            <h3>Flamegraphs</h3>
            <p class="performance-panel-caption">${formatNumber.format(available)}/${formatNumber.format(profiles.length)} profiles available for this promise.</p>
            <div class="flamegraph-layout" data-flamegraph-scenario="${escapeHtml(scenario.id)}">
                <aside class="panel-card flamegraph-sidebar">
                    <div class="panel-card__header">
                        <div>
                            <h2>Profiles</h2>
                            <p>Select a profile to view its flamegraph.</p>
                        </div>
                    </div>
                    <div class="flamegraph-list" data-flamegraph-list="${escapeHtml(scenario.id)}"></div>
                </aside>
                <div class="panel-card flamegraph-main">
                    <div class="flamegraph-content" data-flamegraph-content="${escapeHtml(scenario.id)}">
                        <p class="muted">Select a profile from the sidebar to view the flamegraph.</p>
                    </div>
                </div>
            </div>
        </section>`;
    }

    function renderScenarioLatencyEvidence(rows, options = {}) {
        const over = rows.filter((item) => item.over_budget).length;
        const body = rows.map((item) => {
            const ratio = item.budget_ms ? Number(item.mean_ms || 0) / Number(item.budget_ms || 1) : 0;
            return `<tr>
                <td><code>${escapeHtml(item.label || item.id || "-")}</code></td>
                <td><span class="pill">${escapeHtml(item.family || "unmapped")}</span></td>
                <td class="${item.over_budget ? "risk-bad" : "risk-good"}">${formatMs(item.mean_ms)}</td>
                <td>${formatMs(item.budget_ms)}</td>
                <td><span class="status-pill status-pill--${item.over_budget ? "bad" : ratio >= 0.85 ? "watch" : "ok"}">${escapeHtml(ratio ? formatRatio(ratio) : "-")}</span></td>
                <td>${renderPills(item.signals || [])}</td>
                <td>${renderScenarioProfileLinks(item.matching_flamegraphs || [])}</td>
            </tr>`;
        });
        return renderScenarioEvidenceTable({
            title: "Targeted Latency Paths",
            caption: `${formatNumber.format(rows.length)} change-validation rows - ${formatNumber.format(over)} over budget`,
            open: true,
            headers: ["Test", "Family", "Mean", "Budget", "Ratio", "Signals", "Profiles"],
            rows: body,
            lead: options.includeChart === false ? "" : renderScenarioLatencyChart(rows, options.scenario),
        });
    }

    function renderScenarioLatencyChart(rows, scenario = null) {
        const series = buildScenarioLatencySeries(rows);
        if (!series.length) return "";
        const targetValues = [...new Set(rows.map((row) => Number(row.budget_ms || 0)).filter((value) => Number.isFinite(value) && value > 0))]
            .sort((left, right) => left - right)
            .slice(0, 4);
        const points = series.flatMap((entry) => entry.points);
        const xValues = points.map((point) => point.x).filter((value) => value > 0);
        const yValues = points.map((point) => point.meanMs).concat(targetValues).filter((value) => value > 0);
        const width = 760;
        const height = 300;
        const left = 62;
        const right = 34;
        const top = 24;
        const bottom = 52;
        const plotWidth = width - left - right;
        const plotHeight = height - top - bottom;
        const xMin = Math.min(...xValues);
        const xMax = Math.max(...xValues);
        const logXMin = Math.log10(Math.max(xMin, 1));
        const logXMax = Math.log10(Math.max(xMax, xMin + 1));
        const xPosition = (value) => {
            if (xMin === xMax) return left + plotWidth / 2;
            const ratio = (Math.log10(Math.max(value, 1)) - logXMin) / Math.max(logXMax - logXMin, 0.0001);
            return left + ratio * plotWidth;
        };
        const yTicks = buildLogTicks(Math.min(...yValues), Math.max(...yValues));
        const yMin = yTicks[0];
        const yMax = yTicks[yTicks.length - 1];
        const logYMin = Math.log10(yMin);
        const logYMax = Math.log10(yMax);
        const yPosition = (value) => {
            const ratio = (Math.log10(Math.max(value, yMin)) - logYMin) / Math.max(logYMax - logYMin, 0.0001);
            return top + plotHeight - ratio * plotHeight;
        };
        const xTicks = selectAxisTicks([...new Set(xValues)].sort((leftValue, rightValue) => leftValue - rightValue), 6)
            .map((value) => {
                const x = xPosition(value);
                return `<g>
                    <line class="chart-axis-line" x1="${x}" y1="${height - bottom}" x2="${x}" y2="${height - bottom + 6}"></line>
                    <text class="chart-tick-label" x="${x}" y="${height - bottom + 22}" text-anchor="middle">${escapeHtml(formatStressLabel(value))}</text>
                </g>`;
            }).join("");
        const yGrid = yTicks.map((tick) => {
            const y = yPosition(tick);
            return `<g>
                <line class="chart-grid-line" x1="${left}" y1="${y}" x2="${width - right}" y2="${y}"></line>
                <text class="chart-tick-label" x="${left - 10}" y="${y + 4}" text-anchor="end">${escapeHtml(formatAxisMs(tick))}</text>
            </g>`;
        }).join("");
        const targets = targetValues.map((target) => {
            const y = yPosition(target);
            return `<g>
                <line class="scenario-latency-target" x1="${left}" y1="${y}" x2="${width - right}" y2="${y}"></line>
                <text class="chart-tick-label scenario-latency-target-label" x="${width - right + 4}" y="${y + 4}">${escapeHtml(formatMs(target))}</text>
            </g>`;
        }).join("");
        const promiseColor = performancePromiseColor(scenario?.id || performancePromiseForItem(rows[0] || {}).id);
        const coloredSeries = withDistinctSeriesColors(series.map((entry) => ({ ...entry, color: promiseColor })));
        const seriesMarkup = coloredSeries.map((entry) => {
            const color = entry.color;
            const path = entry.points.map((point, pointIndex) => `${pointIndex ? "L" : "M"} ${xPosition(point.x).toFixed(1)} ${yPosition(point.meanMs).toFixed(1)}`).join(" ");
            const markers = entry.points.map((point) => {
                const x = xPosition(point.x);
                const y = yPosition(point.meanMs);
                const over = point.budgetMs && point.meanMs > point.budgetMs;
                return `<circle class="scenario-latency-point ${over ? "is-over" : ""}" cx="${x.toFixed(1)}" cy="${y.toFixed(1)}" r="4.5" stroke="${color}"><title>${escapeHtml(entry.label)} ${escapeHtml(formatStressLabel(point.x))}: ${escapeHtml(formatMs(point.meanMs))}</title></circle>`;
            }).join("");
            return `<g>
                <path class="scenario-latency-line" d="${path}" stroke="${color}"></path>
                ${markers}
            </g>`;
        }).join("");
        const legend = coloredSeries.map((entry) => {
            const color = entry.color;
            return `<span class="scenario-latency-legend__item"><i style="background:${color}"></i>${escapeHtml(entry.label)}</span>`;
        }).join("");
        return `<div class="scenario-latency-chart">
            <div class="scenario-latency-chart__header">
                <strong>Latency Under Stress</strong>
                <span>Each line is a test family. Dashed guide${targetValues.length > 1 ? "s are" : " is"} target latency.</span>
            </div>
            <div class="scenario-latency-chart__frame">
                <svg class="scenario-latency-svg" viewBox="0 0 ${width} ${height}" role="img" aria-label="Latency degradation under stress">
                    ${yGrid}
                    ${targets}
                    <line class="chart-axis-line" x1="${left}" y1="${top}" x2="${left}" y2="${height - bottom}"></line>
                    <line class="chart-axis-line" x1="${left}" y1="${height - bottom}" x2="${width - right}" y2="${height - bottom}"></line>
                    ${xTicks}
                    ${seriesMarkup}
                    <text class="chart-axis-label" x="${width / 2}" y="${height - 12}" text-anchor="middle">Stress</text>
                    <text class="chart-axis-label" x="18" y="${top + plotHeight / 2}" text-anchor="middle" transform="rotate(-90 18 ${top + plotHeight / 2})">Time (ms, log scale)</text>
                </svg>
            </div>
            <div class="scenario-latency-legend">${legend}</div>
        </div>`;
    }

    function buildScenarioLatencySeries(rows) {
        const seen = new Set();
        const groups = new Map();
        rows.forEach((row) => {
            const point = parseLatencyStressPoint(row);
            if (!point) return;
            const key = `${point.base}:${point.x}:${Number(row.mean_ms || 0).toFixed(6)}:${Number(row.budget_ms || 0).toFixed(3)}`;
            if (seen.has(key)) return;
            seen.add(key);
            if (!groups.has(point.base)) {
                groups.set(point.base, []);
            }
            groups.get(point.base).push({
                x: point.x,
                meanMs: Number(row.mean_ms || 0),
                budgetMs: Number(row.budget_ms || 0),
            });
        });
        return [...groups.entries()]
            .map(([base, points]) => ({
                key: base,
                label: latencySeriesLabel(base),
                points: points.sort((left, right) => left.x - right.x),
            }))
            .filter((entry) => entry.points.length >= 2)
            .sort((left, right) => right.points[right.points.length - 1].meanMs - left.points[left.points.length - 1].meanMs)
            .slice(0, 8);
    }

    function parseLatencyStressPoint(row) {
        const raw = String(row.id || row.label || "");
        const match = raw.match(/^(.+)\/([^/]+)$/);
        if (!match) return null;
        const x = Number(match[2]);
        if (!Number.isFinite(x) || x <= 0) return null;
        return { base: match[1], x };
    }

    function latencySeriesLabel(value) {
        return titleCaseMetricName(value)
            .replace(/\bLatency\b/g, "")
            .replace(/\bAggregate Size\b/g, "")
            .replace(/\bCompletion\b/g, "")
            .replace(/\bCurrent App State\b/g, "App State")
            .replace(/\s+/g, " ")
            .trim();
    }

    function selectAxisTicks(values, limit) {
        if (values.length <= limit) return values;
        const step = (values.length - 1) / (limit - 1);
        return Array.from({ length: limit }, (_, index) => values[Math.round(index * step)]);
    }

    function formatStressLabel(value) {
        const numeric = Number(value || 0);
        if (numeric >= 1_000_000) return `${formatNumber.format(numeric / 1_000_000)}M`;
        if (numeric >= 1_000) return `${formatNumber.format(numeric / 1_000)}K`;
        return formatNumber.format(numeric);
    }

    function renderScenarioCapacityEvidence(rows, options = {}) {
        const ceilings = rows.filter((item) => item.ceiling_reached).length;
        const body = rows.map((item) => `<tr>
            <td><code>${escapeHtml(item.label || item.id || "-")}</code></td>
            <td><span class="pill">${escapeHtml(item.failure_mode || "not_reached")}</span></td>
            <td>${escapeHtml(item.last_successful_label || "-")}</td>
            <td>${escapeHtml(item.first_failure_label || "-")}</td>
            <td><span class="pill">${escapeHtml(item.suspected_limiting_resource || "cpu")}</span></td>
            <td>${escapeHtml(formatBytes(item.peak_working_set_bytes))}</td>
            <td>${renderScenarioProfileLinks(item.matching_flamegraphs || [])}</td>
        </tr>`);
        return renderScenarioEvidenceTable({
            title: "Ceiling Promise Health",
            caption: `${formatNumber.format(rows.length)} health checks - ${formatNumber.format(ceilings)} ceilings reached`,
            open: rows.length > 0,
            headers: ["Check", "Failure mode", "Last OK", "First failure", "Resource", "Peak working set", "Profiles"],
            rows: body,
            lead: options.includeChart === false ? "" : renderScenarioCapacityChart(rows, options.scenario),
        });
    }

    function renderScenarioCapacityChart(rows, scenario = null) {
        const series = buildScenarioCapacitySeries(rows);
        if (!series.length) return "";
        const points = series.flatMap((entry) => entry.points);
        const xValues = points.map((point) => point.x).filter((value) => value > 0);
        const yValues = points.map((point) => point.elapsedMs)
            .concat(series.map((entry) => entry.thresholdMs).filter((value) => value > 0))
            .filter((value) => value > 0);
        if (!xValues.length || !yValues.length) return "";
        const width = 760;
        const height = 300;
        const left = 62;
        const right = 34;
        const top = 24;
        const bottom = 52;
        const plotWidth = width - left - right;
        const plotHeight = height - top - bottom;
        const xMin = Math.min(...xValues);
        const xMax = Math.max(...xValues);
        const logXMin = Math.log10(Math.max(xMin, 1));
        const logXMax = Math.log10(Math.max(xMax, xMin + 1));
        const xPosition = (value) => {
            if (xMin === xMax) return left + plotWidth / 2;
            const ratio = (Math.log10(Math.max(value, 1)) - logXMin) / Math.max(logXMax - logXMin, 0.0001);
            return left + ratio * plotWidth;
        };
        const yTicks = buildLogTicks(Math.min(...yValues), Math.max(...yValues));
        const yMin = yTicks[0];
        const yMax = yTicks[yTicks.length - 1];
        const logYMin = Math.log10(yMin);
        const logYMax = Math.log10(yMax);
        const yPosition = (value) => {
            const ratio = (Math.log10(Math.max(value, yMin)) - logYMin) / Math.max(logYMax - logYMin, 0.0001);
            return top + plotHeight - ratio * plotHeight;
        };
        const xLabelByValue = new Map(points.map((point) => [point.x, point.xLabel || formatStressLabel(point.x)]));
        const xTicks = selectAxisTicks([...new Set(xValues)].sort((leftValue, rightValue) => leftValue - rightValue), 6)
            .map((value) => {
                const x = xPosition(value);
                return `<g>
                    <line class="chart-axis-line" x1="${x}" y1="${height - bottom}" x2="${x}" y2="${height - bottom + 6}"></line>
                    <text class="chart-tick-label" x="${x}" y="${height - bottom + 22}" text-anchor="middle">${escapeHtml(xLabelByValue.get(value) || formatStressLabel(value))}</text>
                </g>`;
            }).join("");
        const yGrid = yTicks.map((tick) => {
            const y = yPosition(tick);
            return `<g>
                <line class="chart-grid-line" x1="${left}" y1="${y}" x2="${width - right}" y2="${y}"></line>
                <text class="chart-tick-label" x="${left - 10}" y="${y + 4}" text-anchor="end">${escapeHtml(formatAxisMs(tick))}</text>
            </g>`;
        }).join("");
        const targetValues = capacityChartTargetValues(series);
        const targets = targetValues.map((target) => {
            const y = yPosition(target);
            return `<g>
                <line class="scenario-latency-target scenario-capacity-target" x1="${left}" y1="${y}" x2="${width - right}" y2="${y}"></line>
                <text class="chart-tick-label scenario-latency-target-label" x="${width - right + 4}" y="${y + 4}">${escapeHtml(formatMs(target))}</text>
            </g>`;
        }).join("");
        const promiseColor = performancePromiseColor(scenario?.id || performancePromiseForItem(rows[0] || {}).id);
        const coloredSeries = withDistinctSeriesColors(series.map((entry) => ({ ...entry, color: promiseColor })));
        const seriesMarkup = coloredSeries.map((entry) => {
            const color = entry.color;
            const path = entry.points.map((point, pointIndex) => `${pointIndex ? "L" : "M"} ${xPosition(point.x).toFixed(1)} ${yPosition(point.elapsedMs).toFixed(1)}`).join(" ");
            const markers = entry.points.map((point) => {
                const x = xPosition(point.x);
                const y = yPosition(point.elapsedMs);
                const over = entry.thresholdMs && point.elapsedMs > entry.thresholdMs;
                return `<circle class="scenario-latency-point scenario-capacity-point ${over ? "is-over" : ""}" cx="${x.toFixed(1)}" cy="${y.toFixed(1)}" r="4.5" stroke="${color}"><title>${escapeHtml(entry.label)} ${escapeHtml(point.xLabel)}: ${escapeHtml(formatMs(point.elapsedMs))}</title></circle>`;
            }).join("");
            return `<g>
                <path class="scenario-latency-line scenario-capacity-line" d="${path}" stroke="${color}"></path>
                ${markers}
            </g>`;
        }).join("");
        const legend = coloredSeries.map((entry) => {
            const color = entry.color;
            return `<span class="scenario-latency-legend__item"><i style="background:${color}"></i>${escapeHtml(entry.label)}</span>`;
        }).join("");
        return `<div class="scenario-latency-chart scenario-capacity-chart">
            <div class="scenario-latency-chart__header">
                <strong>Performance Against Size</strong>
                <span>Elapsed time as workload size grows. Dotted guide${targetValues.length > 1 ? "s are" : " is"} target time.</span>
            </div>
            <div class="scenario-latency-chart__frame">
                <svg class="scenario-latency-svg" viewBox="0 0 ${width} ${height}" role="img" aria-label="Capacity performance against workload size">
                    ${yGrid}
                    ${targets}
                    <line class="chart-axis-line" x1="${left}" y1="${top}" x2="${left}" y2="${height - bottom}"></line>
                    <line class="chart-axis-line" x1="${left}" y1="${height - bottom}" x2="${width - right}" y2="${height - bottom}"></line>
                    ${xTicks}
                    ${seriesMarkup}
                    <text class="chart-axis-label" x="${width / 2}" y="${height - 12}" text-anchor="middle">Size</text>
                    <text class="chart-axis-label" x="18" y="${top + plotHeight / 2}" text-anchor="middle" transform="rotate(-90 18 ${top + plotHeight / 2})">Elapsed (ms, log scale)</text>
                </svg>
            </div>
            <div class="scenario-latency-legend">${legend}</div>
        </div>`;
    }

    function capacityChartTargetValues(series) {
        const values = [...new Set(series.map((entry) => entry.thresholdMs).filter((value) => value > 0))]
            .sort((leftValue, rightValue) => leftValue - rightValue);
        if (values.length <= 1) return values;

        const lowest = values[0];
        const highest = values[values.length - 1];
        if (lowest > 0 && highest / lowest <= 1.15) {
            return [highest];
        }
        return values.slice(0, 4);
    }

    function buildScenarioCapacitySeries(rows) {
        return rows
            .map((row) => {
                const detail = capacityScenarioDetail(row);
                const samples = (detail?.samples || [])
                    .map((sample) => ({
                        x: Number(sample.workload_value || 0),
                        xLabel: sample.workload_label || formatStressLabel(sample.workload_value),
                        elapsedMs: Number(sample.elapsed_ms || 0),
                    }))
                    .filter((sample) => Number.isFinite(sample.x) && sample.x > 0 && Number.isFinite(sample.elapsedMs) && sample.elapsedMs > 0)
                    .sort((left, right) => left.x - right.x);
                if (samples.length < 2) return null;
                return {
                    key: row.id || detail?.scenario || row.label,
                    label: compactScenarioLabel(detail || row),
                    thresholdMs: Number(detail?.threshold_ms || row.threshold_ms || 0),
                    points: samples,
                };
            })
            .filter(Boolean)
            .sort((left, right) => right.points[right.points.length - 1].elapsedMs - left.points[left.points.length - 1].elapsedMs)
            .slice(0, 8);
    }

    function capacityScenarioDetail(row) {
        const scenarios = state.capacityReport?.scenarios || [];
        return scenarios.find((item) => item.scenario === row.id)
            || scenarios.find((item) => item.scenario_label === row.label)
            || scenarios.find((item) => item.scenario === row.scenario);
    }

    function renderScenarioResourceEvidence(rows, options = {}) {
        const worstElapsed = maxMetric(rows, "max_elapsed_ms");
        const body = rows.map((item) => `<tr>
            <td><code>${escapeHtml(item.label || item.id || "-")}</code></td>
            <td><span class="pill">${escapeHtml(item.focus || "resource")}</span></td>
            <td>${formatNumber.format(item.sample_count || 0)}</td>
            <td>${formatMs(item.max_elapsed_ms)}</td>
            <td>${escapeHtml(formatBytes(item.max_peak_live_bytes))}</td>
            <td>${escapeHtml(formatBytes(item.max_working_set_bytes))}</td>
            <td>${item.page_fault_growth == null ? "-" : formatNumber.format(item.page_fault_growth)}</td>
            <td>${item.handle_growth == null ? "-" : formatNumber.format(item.handle_growth)}</td>
        </tr>`);
        return renderScenarioEvidenceTable({
            title: "Targeted Resource Paths",
            caption: `${formatNumber.format(rows.length)} change-validation probes - worst elapsed ${worstElapsed ? formatMs(worstElapsed) : "-"}`,
            open: options.open ?? (rows.length > 0),
            headers: ["Probe", "Focus", "Samples", "Max elapsed", "Peak live", "Working set", "PF growth", "Handle growth"],
            rows: body,
        });
    }

    function renderScenarioProfileEvidence(rows, options = {}) {
        const available = rows.filter((item) => item.available).length;
        const body = rows.map((item) => `<tr>
            <td><code>${escapeHtml(item.name || item.id || "-")}</code></td>
            <td class="${item.available ? "risk-good" : "risk-bad"}">${escapeHtml(item.available ? "available" : "missing")}</td>
            <td>${renderPills(item.families || [])}</td>
            <td>${renderPills(item.benchmark_keys || [])}</td>
            <td>${item.available ? '<span class="muted">available</span>' : escapeHtml(item.issue || "-")}</td>
        </tr>`);
        return renderScenarioEvidenceTable({
            title: options.title || "Profiles",
            caption: `${formatNumber.format(available)}/${formatNumber.format(rows.length)} available`,
            open: options.open ?? (rows.length > 0),
            headers: ["Profile", "Status", "Families", "Benchmarks", "Action"],
            rows: body,
            tail: options.tail || "",
        });
    }

    function renderScenarioImplementationEvidence(rows, options = {}) {
        const body = rows.map((item) => `<tr>
            <td><span class="pill">${escapeHtml(item.kind || "-")}</span></td>
            <td><code>${escapeHtml(item.label || "-")}</code></td>
            <td>${escapeHtml(item.measurement || "-")}</td>
            <td>${escapeHtml(item.status || "-")}</td>
            <td>${escapeHtml(item.detail || "-")}</td>
        </tr>`);
        return renderScenarioEvidenceTable({
            title: "Implementations Audit",
            caption: `${formatNumber.format(rows.length)} measurements`,
            open: options.open ?? false,
            headers: ["Kind", "Label", "Measurement", "Status", "Detail"],
            rows: body,
        });
    }

    function renderScenarioEvidenceTable({ title, caption, open, headers, rows, lead = "", tail = "" }) {
        return `<details class="promise-evidence" ${open ? "open" : ""}>
            <summary><span>${escapeHtml(title)}</span><em>${escapeHtml(caption)}</em></summary>
            ${lead}
            <div class="promise-evidence__table">
                ${renderInlineTable(headers, rows, "No evidence rows loaded.")}
            </div>
            ${tail}
        </details>`;
    }

    function renderInlineTable(headers, rows, emptyMessage) {
        return `<table>
            <thead><tr>${headers.map((header) => `<th>${escapeHtml(header)}</th>`).join("")}</tr></thead>
            <tbody>${rows.length ? rows.join("") : `<tr><td colspan="${headers.length}" class="muted">${escapeHtml(emptyMessage)}</td></tr>`}</tbody>
        </table>`;
    }

    function renderScenarioProfileLinks(values) {
        const profiles = pillValues(values);
        if (!profiles.length) return '<span class="muted">-</span>';
        return renderPills(profiles);
    }

    function scenarioFamilies(scenario) {
        const families = (scenario.implementations || [])
            .flatMap((item) => [item.family, item.workload_family, ...(item.families || [])])
            .concat(Object.values(scenario.evidence || {})
                .flatMap((rows) => Array.isArray(rows) ? rows : [])
                .flatMap((item) => [item.family, item.workload_family, ...(item.families || [])]))
            .filter(Boolean);
        return [...new Set(families)].sort();
    }

    function scenarioStatus(scenario) {
        const checks = scenario.scale_checks || [];
        const missingScale = checks.some((check) => !check.met);
        const misses = Number(scenario.budget_misses || 0);
        if (!scenario.implementation_count && scenario.coverage_status !== "covered") {
            return { label: "Untested", cls: "stale" };
        }
        if (missingScale) {
            return { label: "Below target", cls: "bad" };
        }
        if (misses > 0) {
            return { label: "Watch", cls: "watch" };
        }
        return { label: "Met", cls: "ok" };
    }

    function bestScaleCheck(scenario) {
        const checks = scenario.scale_checks || [];
        return checks.find((check) => !check.met) || checks[0] || null;
    }

    function formatScaleValue(value, unit) {
        if (value == null || value === "") {
            return "-";
        }
        if (unit === "bytes") {
            return formatBytes(Number(value));
        }
        if (unit === "ms") {
            return `${formatNumber.format(Number(value))} ms`;
        }
        if (unit === "rows") {
            return `${formatNumber.format(Number(value))} rows`;
        }
        return formatNumber.format(Number(value));
    }

    function renderPerformanceOverview() {
        const target = byId("performance-verdict");
        if (!target) return;
        const digest = computePerformanceDigest();
        const worst = digest.worstOverBudgetRow;
        const worstRatio = worst ? budgetRatio(worst) : 0;
        const scenarioCls = digest.scenarioTotal && digest.scenariosMet === digest.scenarioTotal ? "ok" : "watch";
        const cells = [
            { label: "Verdict", value: `${formatNumber.format(digest.scenariosMet)} of ${formatNumber.format(digest.scenarioTotal)} promises met`, cls: scenarioCls, sentence: true },
            { label: "Budget misses", value: formatNumber.format(digest.summaryBudgetMisses), cls: digest.summaryBudgetMisses ? "bad" : "ok" },
            { label: "Near ceilings", value: formatNumber.format(digest.nearCeilings), cls: digest.nearCeilings ? "watch" : "ok" },
            {
                label: "Measured gaps",
                value: `${formatNumber.format(digest.measurementGapsClosed)} of 6 closed`,
                cls: digest.measurementGapsClosed >= 6 ? "ok" : "watch",
            },
            {
                label: "Worst latency",
                value: worst && worstRatio > 1 ? `${formatMs(latencyMs(worst))} (${formatRatio(worstRatio)} budget)` : "All within budget",
                cls: worst && worstRatio > 1 ? "bad" : "ok",
            },
            { label: "Peak working set", value: digest.peakWorkingSet ? formatBytes(digest.peakWorkingSet) : "-", cls: "neutral" },
        ];
        target.innerHTML = `<section class="performance-headline-strip" aria-label="Performance headline results" data-jump-target="performance-headline-charts">
            ${cells.map((cell) => `<div class="performance-headline-cell performance-headline-cell--${cell.cls}${cell.sentence ? " performance-headline-cell--sentence" : ""}">
                <span>${escapeHtml(cell.label)}</span>
                <strong>${escapeHtml(cell.value)}</strong>
            </div>`).join("")}
        </section>`;
    }

    function renderPerformanceMeasurementGaps() {
        const target = byId("performance-measurement-gaps");
        if (!target) return;
        const rows = measurementGapRows();
        const tabPhaseRow = targetedTabPhaseRow();
        if (tabPhaseRow) rows.push(tabPhaseRow);
        if (!rows.length) {
            target.innerHTML = `<p class="muted">No measurement-gap resource probes loaded.</p>`;
            return;
        }
        target.innerHTML = rows.map((row, index) => {
            return `<article class="performance-gap-card" style="--promise-color:${escapeHtml(row.color)}" aria-label="${escapeHtml(row.title)} measurement graph">
                <div class="performance-gap-card__header">
                    <span class="rank-pill">${index + 1}</span>
                    <div>
                        <h3>${escapeHtml(row.title)}</h3>
                        <p>${escapeHtml(row.subtitle)}</p>
                    </div>
                    <span class="status-pill status-pill--ok" title="Focused probe samples loaded">${escapeHtml(row.badge)}</span>
                </div>
                ${row.chartKind === "tab-phase" ? renderTargetedTabPhaseChart(row) : renderMeasurementGapChart(row)}
                <div class="performance-gap-card__metrics">
                    <span><strong>${escapeHtml(formatMs(row.maxElapsedMs))}</strong><em>max elapsed</em></span>
                    <span><strong>${escapeHtml(formatBytes(row.maxPeakBytes))}</strong><em>peak allocation</em></span>
                    <span><strong>${escapeHtml(row.maxWorkloadLabel)}</strong><em>largest run</em></span>
                </div>
            </article>`;
        }).join("");
    }

    function measurementGapRows() {
        const order = [
            "peak RSS / allocator high-water mark during very large UTF-8 load",
            "edited-buffer search preview rendering with many matches and many pieces",
            "provenance-store retained memory after hundreds of thousands of edits and history-budget eviction",
            "anchor-heavy editing with many views, selections, search results, and scroll anchors",
            "fragmented-buffer paste/cut/undo/redo after long sessions",
            "session persistence broken down into snapshot cost, serialization cost, file I/O, and restore reconstruction",
        ];
        const grouped = new Map();
        (state.resourceProfiles?.scenarios || [])
            .filter((item) => item.measurement_gap)
            .forEach((item) => {
                const key = item.measurement_gap;
                if (!grouped.has(key)) grouped.set(key, []);
                grouped.get(key).push(item);
            });

        return order.map((gap) => {
            const rows = grouped.get(gap) || [];
            if (!rows.length) return null;
            const representative = maxBy(rows, (row) => Number(row.max_elapsed_ms || 0)) || rows[0];
            const samples = rows.flatMap((row) => row.samples || []);
            const largestSample = maxBy(samples, (sample) => Number(sample.workload_value || 0));
            const elapsed = Math.max(...rows.map((row) => Number(row.max_elapsed_ms || 0)), 0);
            const peak = Math.max(...rows.map((row) => Number(row.max_peak_live_bytes || row.max_working_set_bytes || 0)), 0);
            const promise = performancePromiseForItem(representative);
            const series = samples
                .map((sample, index) => ({
                    index,
                    workloadValue: Number(sample.workload_value || index + 1),
                    workloadLabel: sample.workload_label || `sample ${index + 1}`,
                    elapsedMs: Number(sample.elapsed_ms || 0),
                    peakBytes: Number(sample.peak_live_bytes || sample.working_set_bytes || 0),
                    resultLabel: sample.result_label || "",
                }))
                .filter((sample) => sample.elapsedMs > 0 || sample.peakBytes > 0)
                .sort((left, right) => left.workloadValue - right.workloadValue || left.index - right.index);
            return {
                gap,
                title: measurementGapTitle(gap),
                subtitle: compactScenarioLabel(representative),
                color: promise.color,
                badge: `${series.length || rows.length} samples`,
                maxElapsedMs: elapsed,
                maxPeakBytes: peak,
                maxWorkloadLabel: largestSample?.workload_label || "-",
                series,
            };
        }).filter(Boolean);
    }

    function targetedTabPhaseRow() {
        const phaseSpecs = [
            { id: "tab_build_targeted", label: "Build", color: "#6fd0ff" },
            { id: "tab_split_targeted", label: "Split", color: "#f3c969" },
            { id: "tab_combine_targeted", label: "Combine", color: "#7ddc9b" },
            { id: "startup_visible_restore_cost", label: "Visible", color: "#ff9f7a" },
            { id: "session_restore_cost", label: "Hydrate all", color: "#c7a6ff" },
        ];
        const scenarios = state.resourceProfiles?.scenarios || [];
        const seriesGroups = phaseSpecs
            .map((phase) => {
                const scenario = scenarios.find((item) => item.scenario === phase.id);
                const points = (scenario?.samples || [])
                    .map((sample, index) => ({
                        index,
                        workloadValue: Number(sample.workload_value || index + 1),
                        workloadLabel: sample.workload_label || `sample ${index + 1}`,
                        elapsedMs: Number(sample.elapsed_ms || 0),
                        peakBytes: Number(sample.peak_live_bytes || sample.working_set_bytes || 0),
                        resultLabel: sample.result_label || "",
                    }))
                    .filter((sample) => sample.elapsedMs > 0 || sample.peakBytes > 0)
                    .sort((left, right) => left.workloadValue - right.workloadValue || left.index - right.index);
                if (!points.length) return null;
                return {
                    ...phase,
                    scenario,
                    points,
                };
            })
            .filter(Boolean);
        if (!seriesGroups.length) return null;
        const allPoints = seriesGroups.flatMap((group) => group.points);
        const allScenarios = seriesGroups.map((group) => group.scenario).filter(Boolean);
        const largestSample = maxBy(allPoints, (sample) => Number(sample.workloadValue || 0));
        return {
            chartKind: "tab-phase",
            title: "Tab Count Phase Breakdown",
            subtitle: "Build, manipulate, time-to-visible, and full hydration probes share one graph.",
            color: performancePromiseForItem({ workload_family: "tab-management" }).color,
            badge: `${seriesGroups.length} phases`,
            maxElapsedMs: Math.max(...allPoints.map((point) => point.elapsedMs), 0),
            maxPeakBytes: Math.max(
                ...allScenarios.map((scenario) => Number(scenario.max_peak_live_bytes || scenario.max_working_set_bytes || 0)),
                ...allPoints.map((point) => point.peakBytes || 0),
                0
            ),
            maxWorkloadLabel: largestSample?.workloadLabel || "-",
            seriesGroups,
        };
    }

    function measurementGapTitle(gap) {
        const titles = {
            "peak RSS / allocator high-water mark during very large UTF-8 load": "Large UTF-8 Load Memory",
            "edited-buffer search preview rendering with many matches and many pieces": "Edited Search Previews",
            "provenance-store retained memory after hundreds of thousands of edits and history-budget eviction": "Provenance Retention",
            "anchor-heavy editing with many views, selections, search results, and scroll anchors": "Anchor-Heavy Editing",
            "fragmented-buffer paste/cut/undo/redo after long sessions": "Fragmented Mutation",
            "session persistence broken down into snapshot cost, serialization cost, file I/O, and restore reconstruction": "Session Stage Costs",
        };
        return titles[gap] || titleCaseMetricName(gap);
    }

    function renderMeasurementGapChart(row) {
        const width = 460;
        const height = 170;
        const left = 42;
        const right = 16;
        const top = 18;
        const bottom = 34;
        const plotWidth = width - left - right;
        const plotHeight = height - top - bottom;
        const samples = row.series || [];
        if (!samples.length) {
            return `<div class="performance-gap-chart performance-gap-chart--empty">No probe samples loaded.</div>`;
        }
        const maxElapsed = Math.max(...samples.map((sample) => sample.elapsedMs), 1);
        const maxPeak = Math.max(...samples.map((sample) => sample.peakBytes), 1);
        const xFor = (index) => left + (plotWidth * (index + 0.5)) / samples.length;
        const elapsedY = (value) => top + plotHeight - (value / maxElapsed) * plotHeight;
        const peakY = (value) => top + plotHeight - (value / maxPeak) * plotHeight;
        const barWidth = Math.max(10, Math.min(36, plotWidth / samples.length * 0.48));
        const bars = samples.map((sample, index) => {
            const x = xFor(index) - barWidth / 2;
            const y = elapsedY(sample.elapsedMs);
            const h = top + plotHeight - y;
            const title = `${sample.workloadLabel}: ${formatMs(sample.elapsedMs)}, ${formatBytes(sample.peakBytes)} peak`;
            return `<rect x="${x.toFixed(1)}" y="${y.toFixed(1)}" width="${barWidth.toFixed(1)}" height="${Math.max(2, h).toFixed(1)}"><title>${escapeHtml(title)}</title></rect>`;
        }).join("");
        const linePoints = samples
            .map((sample, index) => `${xFor(index).toFixed(1)},${peakY(sample.peakBytes).toFixed(1)}`)
            .join(" ");
        const markers = samples.map((sample, index) => {
            const title = `${sample.workloadLabel}: ${formatBytes(sample.peakBytes)} peak`;
            return `<circle cx="${xFor(index).toFixed(1)}" cy="${peakY(sample.peakBytes).toFixed(1)}" r="3.5"><title>${escapeHtml(title)}</title></circle>`;
        }).join("");
        const labels = samples.map((sample, index) => {
            if (samples.length > 4 && index % Math.ceil(samples.length / 4) !== 0 && index !== samples.length - 1) return "";
            return `<text x="${xFor(index).toFixed(1)}" y="${height - 10}" text-anchor="middle">${escapeHtml(shortWorkloadLabel(sample.workloadLabel))}</text>`;
        }).join("");
        return `<svg class="performance-gap-chart" viewBox="0 0 ${width} ${height}" role="img" aria-label="${escapeHtml(row.title)} elapsed and peak memory chart">
            <g class="performance-gap-chart__grid">
                <line x1="${left}" x2="${width - right}" y1="${top}" y2="${top}"></line>
                <line x1="${left}" x2="${width - right}" y1="${top + plotHeight / 2}" y2="${top + plotHeight / 2}"></line>
                <line x1="${left}" x2="${width - right}" y1="${top + plotHeight}" y2="${top + plotHeight}"></line>
            </g>
            <g class="performance-gap-chart__bars">${bars}</g>
            <polyline class="performance-gap-chart__memory-line" points="${linePoints}"></polyline>
            <g class="performance-gap-chart__memory-points">${markers}</g>
            <g class="performance-gap-chart__labels">${labels}</g>
            <text class="performance-gap-chart__axis" x="${left}" y="12">${escapeHtml(formatMs(maxElapsed))}</text>
            <text class="performance-gap-chart__axis performance-gap-chart__axis--right" x="${width - right}" y="12" text-anchor="end">${escapeHtml(formatBytes(maxPeak))}</text>
        </svg>`;
    }

    function renderTargetedTabPhaseChart(row) {
        const width = 460;
        const height = 190;
        const left = 46;
        const right = 18;
        const top = 18;
        const bottom = 42;
        const plotWidth = width - left - right;
        const plotHeight = height - top - bottom;
        const groups = row.seriesGroups || [];
        const points = groups.flatMap((group) => group.points || []);
        if (!points.length) {
            return `<div class="performance-gap-chart performance-gap-chart--empty">No tab phase samples loaded.</div>`;
        }
        const xValues = [...new Set(points.map((point) => Number(point.workloadValue || 0)).filter((value) => value > 0))]
            .sort((leftValue, rightValue) => leftValue - rightValue);
        const yValues = points.map((point) => Number(point.elapsedMs || 0)).filter((value) => value > 0);
        const xMin = Math.min(...xValues);
        const xMax = Math.max(...xValues);
        const xLogMin = Math.log10(Math.max(xMin, 1));
        const xLogMax = Math.log10(Math.max(xMax, xMin + 1));
        const xFor = (value) => {
            if (xMin === xMax) return left + plotWidth / 2;
            const ratio = (Math.log10(Math.max(value, 1)) - xLogMin) / Math.max(xLogMax - xLogMin, 0.0001);
            return left + ratio * plotWidth;
        };
        const yTicks = buildLogTicks(Math.min(...yValues), Math.max(...yValues));
        const yMin = yTicks[0];
        const yMax = yTicks[yTicks.length - 1];
        const yLogMin = Math.log10(yMin);
        const yLogMax = Math.log10(yMax);
        const yFor = (value) => {
            const ratio = (Math.log10(Math.max(value, yMin)) - yLogMin) / Math.max(yLogMax - yLogMin, 0.0001);
            return top + plotHeight - ratio * plotHeight;
        };
        const yGrid = yTicks.map((tick) => {
            const y = yFor(tick);
            return `<g>
                <line x1="${left}" x2="${width - right}" y1="${y.toFixed(1)}" y2="${y.toFixed(1)}"></line>
                <text x="${left - 8}" y="${(y + 4).toFixed(1)}" text-anchor="end">${escapeHtml(formatAxisMs(tick))}</text>
            </g>`;
        }).join("");
        const xTicks = selectAxisTicks(xValues, 4).map((value) => {
            const label = points.find((point) => point.workloadValue === value)?.workloadLabel || `${value}`;
            const x = xFor(value);
            return `<text x="${x.toFixed(1)}" y="${height - 12}" text-anchor="middle">${escapeHtml(shortWorkloadLabel(label))}</text>`;
        }).join("");
        const series = groups.map((group) => {
            const path = group.points
                .map((point, index) => `${index ? "L" : "M"} ${xFor(point.workloadValue).toFixed(1)} ${yFor(point.elapsedMs).toFixed(1)}`)
                .join(" ");
            const markers = group.points.map((point) => {
                const title = `${group.label} ${point.workloadLabel}: ${formatMs(point.elapsedMs)}`;
                return `<circle cx="${xFor(point.workloadValue).toFixed(1)}" cy="${yFor(point.elapsedMs).toFixed(1)}" r="3.8" stroke="${group.color}"><title>${escapeHtml(title)}</title></circle>`;
            }).join("");
            return `<g>
                <path class="performance-gap-chart__phase-line" d="${path}" stroke="${group.color}"></path>
                ${markers}
            </g>`;
        }).join("");
        const legend = groups.map((group) => `<span><i style="background:${group.color}"></i>${escapeHtml(group.label)}</span>`).join("");
        return `<div>
            <svg class="performance-gap-chart performance-gap-chart--phase" viewBox="0 0 ${width} ${height}" role="img" aria-label="${escapeHtml(row.title)} elapsed phase chart">
                <g class="performance-gap-chart__grid">${yGrid}</g>
                <line class="chart-axis-line" x1="${left}" x2="${left}" y1="${top}" y2="${top + plotHeight}"></line>
                <line class="chart-axis-line" x1="${left}" x2="${width - right}" y1="${top + plotHeight}" y2="${top + plotHeight}"></line>
                ${series}
                <g class="performance-gap-chart__labels">${xTicks}</g>
            </svg>
            <div class="performance-gap-chart__legend">${legend}</div>
        </div>`;
    }

    function shortWorkloadLabel(label) {
        return String(label || "")
            .replace(".0 ", " ")
            .replace("bytes", "B")
            .replace("pieces", "pc")
            .replace("fragments", "frag")
            .replace("anchors", "anc")
            .replace("tabs", "tabs");
    }

    function computePerformanceDigest() {
        const searchRows = state.searchSpeed || [];
        const slowspots = state.slowspots || [];
        const capacityRows = state.capacityReport?.scenarios || [];
        const resourceRows = state.resourceProfiles?.scenarios || [];
        const reviewScenarios = state.performanceReview?.scenarios || [];
        const summary = state.performanceReview?.summary || {};
        const budgetedRows = filteredPerformanceRows(uniquePerformanceRows([...searchRows, ...slowspots]).filter((item) => item.threshold_ms));
        const ratios = budgetedRows.map((item) => ({ item, ratio: budgetRatio(item) })).filter((entry) => Number.isFinite(entry.ratio) && entry.ratio > 0);
        const overBudget = ratios.filter((entry) => entry.ratio > 1).length;
        const watchCount = ratios.filter((entry) => entry.ratio >= 0.85 && entry.ratio <= 1).length;
        const withinBudget = ratios.length - overBudget;
        const sortedRatios = [...ratios].sort((a, b) => b.ratio - a.ratio);
        const healthyRatios = [...ratios].sort((a, b) => a.ratio - b.ratio);
        const scenarioStatuses = reviewScenarios.map(scenarioStatus);
        const scenariosMet = scenarioStatuses.filter((status) => status.cls === "ok").length;
        const memoryBound = capacityRows.filter((item) => (item.suspected_limiting_resource || item.first_saturated_resource) === "memory").length;
        const cpuBound = capacityRows.filter((item) => (item.suspected_limiting_resource || item.first_saturated_resource) === "cpu").length;
        const capacityCeilings = state.capacityReport?.summary?.ceilings_reached ?? capacityRows.filter((item) => item.failure_mode && item.failure_mode !== "not_reached").length;
        const nearCeilings = state.speedReport?.summary?.near_failure_ceilings ?? capacityCeilings;
        const measurementGapsClosed = state.resourceProfiles?.summary?.measurement_gaps_closed
            ?? new Set(resourceRows.filter((item) => item.measurement_gap).map((item) => item.measurement_gap)).size;
        const peakLive = maxBy(resourceRows, (item) => Number(item.max_peak_live_bytes || 0));
        const resourceGrowthRows = topResourceRows(resourceRows, 20).map((row) => ({
            ...row,
            detail: `${row.detail} · ${row.pageFaultsLabel} faults`,
        }));
        return {
            searchRows,
            slowspots,
            capacityRows,
            resourceRows,
            reviewScenarios,
            budgetedRows,
            overBudget,
            watchCount,
            withinBudget,
            worstRow: sortedRatios[0]?.item || null,
            worstOverBudgetRow: sortedRatios.find((entry) => entry.ratio > 1)?.item || null,
            bestRow: healthyRatios[0]?.item || null,
            scenarioTotal: reviewScenarios.length,
            scenariosMet,
            summaryBudgetMisses: summary.budget_misses ?? overBudget,
            nearCeilings,
            measurementGapsClosed,
            capacityCeilings,
            capacityOk: capacityRows.length - capacityCeilings,
            memoryBound,
            cpuBound,
            peakLive,
            peakWorkingSet: Math.max(maxMetric(resourceRows, "max_working_set_bytes"), maxMetric(capacityRows, "peak_working_set_bytes")),
            resourceGrowthRows,
            coveredScenarios: summary.covered_scenarios ?? reviewScenarios.filter((item) => item.coverage_status === "covered").length,
            thinScenarios: summary.thin_scenarios ?? reviewScenarios.filter((item) => item.coverage_status === "thin").length,
            untestedScenarios: reviewScenarios.filter((item) => !item.implementation_count && item.coverage_status !== "covered").length,
            implementationCount: summary.implementation_count ?? reviewScenarios.reduce((sum, item) => sum + Number(item.implementation_count || 0), 0),
            missingScaleTargets: summary.missing_scale_targets ?? reviewScenarios.flatMap((item) => item.scale_checks || []).filter((item) => !item.met).length,
        };
    }

    function filterPerformanceScenarioRows(rows) {
        return (rows || []).filter((item) => {
            return matchesPerformanceDatasetSearch(item);
        });
    }

    function filteredPerformanceRows(rows) {
        return (rows || []).filter((item) => {
            return matchesPerformanceDatasetSearch(item);
        });
    }

    function matchesPerformanceDatasetSearch(item) {
        return matchesFilter(item, state.performanceDatasetSearch || "");
    }

    function uniquePerformanceRows(rows) {
        const seen = new Set();
        return (rows || []).filter((item) => {
            const key = `${item.benchmark_key || ""}:${item.name || ""}:${item.latency_kind || ""}`;
            if (seen.has(key)) return false;
            seen.add(key);
            return true;
        });
    }

    function richerPerformanceRow(left, right) {
        const score = (item) =>
            (item?.scenario_label ? 4 : 0) +
            (item?.parameter_label ? 2 : 0) +
            (item?.sourceLabel ? 1 : 0);
        return score(right) > score(left) ? right : left;
    }

    function distinctPerformanceRows(rows) {
        const byKey = new Map();
        (rows || []).forEach((item) => {
            const key = `${item.benchmark_key || item.name || ""}:${performanceRowParameter(item) || item.name || ""}:${item.latency_kind || ""}`;
            byKey.set(key, byKey.has(key) ? richerPerformanceRow(byKey.get(key), item) : item);
        });
        return [...byKey.values()];
    }

    function latencyMs(item) {
        return Number(item?.mean_ns || 0) / 1_000_000;
    }

    function budgetRatio(item) {
        const threshold = Number(item?.threshold_ms || 0);
        if (!threshold) return 0;
        return latencyMs(item) / threshold;
    }

    function buildPerformanceAnswerChainData() {
        const budgetRows = [
            ...(state.searchSpeed || []).map((item) => ({ ...item, sourceLabel: "Search" })),
            ...(state.slowspots || []).map((item) => ({ ...item, sourceLabel: "Editor & Tabs" })),
        ];
        const filteredBudgetRows = filteredPerformanceRows(budgetRows.filter((item) => item.threshold_ms));
        const budgetPressure = filteredBudgetRows
            .map((item) => {
                const value = budgetRatio(item);
                if (!Number.isFinite(value) || value <= 0) return null;
                const promise = performancePromiseForItem(item);
                return {
                    key: `budget:${item.name || item.benchmark_key || Math.random()}`,
                    label: performanceRowLabel(item),
                    detail: `${item.sourceLabel || "Performance"} - ${formatMs(latencyMs(item))} / ${formatMs(item.threshold_ms)}`,
                    value,
                    valueLabel: formatRatio(value),
                    family: item.workload_family || item.family || "unmapped",
                    resource: item.suspected_limiting_resource || "cpu",
                    profile: (item.matching_flamegraphs || [])[0],
                    promiseId: promise.id,
                    promiseTitle: promise.title,
                    color: promise.color,
                    row: item,
                };
            })
            .filter(Boolean);
        const scalingGrowth = buildPerformanceScalingItems(filteredBudgetRows);
        const capacityHeadroom = buildCapacityHeadroomItems();
        const resourceIntensity = buildResourceIntensityItems();
        const resourceValues = resourceIntensity.map((item) => item.value).filter((value) => Number.isFinite(value) && value > 0).sort((left, right) => left - right);
        const resourceMedian = percentileValue(resourceValues, 0.5) || 1;
        const resourceThreshold = resourceMedian * 2;
        const resourceBounds = {
            min: Math.max(0.000001, Math.min(...resourceValues, resourceMedian) / 1.4),
            max: Math.max(resourceThreshold * 1.3, ...resourceValues, 1),
            scale: "log",
        };
        const pressureGrowth = buildPressureGrowthPoints(scalingGrowth);
        const memoryElapsed = buildMemoryElapsedPoints();
        return {
            budgetPressure,
            scalingGrowth,
            capacityHeadroom,
            resourceIntensity,
            resourceMedian,
            resourceThreshold,
            resourceBounds,
            pressureGrowth,
            pressureBounds: buildGrowthScatterBounds(pressureGrowth),
            memoryElapsed,
            memoryBounds: buildGrowthScatterBounds(memoryElapsed),
        };
    }

    function performanceRowLabel(item) {
        const raw = item?.scenario_label || item?.benchmark_key || item?.name || item?.scenario || "Scenario";
        return titleCaseMetricName(String(raw).replace(/\/\d+$/, ""));
    }

    function performanceRowParameter(item) {
        const direct = Number(item?.parameter_value || 0);
        if (direct > 0) return direct;
        const match = String(item?.name || "").match(/\/(\d+(?:\.\d+)?)$/);
        return match ? Number(match[1]) : 0;
    }

    function performanceRowParameterLabel(item) {
        return item?.parameter_label || (performanceRowParameter(item) ? formatStressLabel(performanceRowParameter(item)) : "-");
    }

    function buildPerformanceScalingItems(rows) {
        const groups = new Map();
        rows.forEach((item) => {
            const x = performanceRowParameter(item);
            if (!x || !item.benchmark_key) return;
            const key = `${item.sourceLabel || "rows"}:${item.benchmark_key}:${item.mode || ""}:${item.latency_kind || ""}`;
            if (!groups.has(key)) groups.set(key, []);
            groups.get(key).push(item);
        });
        return [...groups.entries()]
            .map(([key, group]) => {
                const ordered = [...group]
                    .sort((left, right) => performanceRowParameter(left) - performanceRowParameter(right));
                const points = ordered.map((item) => ({
                    xValue: performanceRowParameter(item),
                    xLabel: performanceRowParameterLabel(item),
                    meanMs: latencyMs(item),
                    thresholdMs: Number(item.threshold_ms || 0),
                    row: item,
                })).filter((point) => point.xValue > 0 && point.meanMs > 0);
                if (points.length < 2) return null;
                const value = calculateDoublingMultiplier(points);
                if (!Number.isFinite(value) || value <= 0) return null;
                const first = points[0];
                const last = points[points.length - 1];
                const worstPoint = [...points].sort((left, right) => (right.meanMs / Math.max(right.thresholdMs, 1)) - (left.meanMs / Math.max(left.thresholdMs, 1)))[0];
                const row = worstPoint?.row || ordered[0];
                const promise = performancePromiseForItem(row);
                return {
                    key: `scaling:${key}`,
                    label: performanceRowLabel(row),
                    detail: `${first.xLabel} -> ${last.xLabel} - ${describeGrowth(value)}`,
                    value,
                    valueLabel: formatRatio(value),
                    family: row.workload_family || row.family || "unmapped",
                    resource: row.suspected_limiting_resource || "cpu",
                    profile: (row.matching_flamegraphs || [])[0],
                    promiseId: promise.id,
                    promiseTitle: promise.title,
                    color: promise.color,
                    sourceRows: ordered,
                    row,
                };
            })
            .filter(Boolean);
    }

    function buildCapacityHeadroomItems() {
        return filterPerformanceScenarioRows(state.capacityReport?.scenarios || [])
            .map((item) => {
                const target = capacityTargetForScenario(item);
                const lastOk = Number(item.last_successful_workload || 0);
                const firstFailure = Number(item.first_failure_workload || 0);
                const failed = Boolean(item.ceiling_reached || firstFailure);
                const observed = failed && firstFailure ? firstFailure : lastOk;
                const value = target.value > 0 && observed > 0 ? observed / target.value : 0;
                if (!Number.isFinite(value) || value <= 0) return null;
                const failureMode = item.failure_mode || "not_reached";
                const promise = performancePromiseForItem(item);
                return {
                    key: `capacity:${item.scenario || item.scenario_label}`,
                    label: compactScenarioLabel(item),
                    detail: `${item.last_successful_label || "-"} -> ${item.first_failure_label || "no ceiling"} - target ${target.label}`,
                    value,
                    valueLabel: formatRatio(value),
                    failed,
                    failureMode,
                    family: item.workload_family || "capacity",
                    resource: item.suspected_limiting_resource || item.first_saturated_resource || "cpu",
                    profile: (item.matching_flamegraphs || [])[0],
                    promiseId: promise.id,
                    promiseTitle: promise.title,
                    color: promise.color,
                    row: item,
                };
            })
            .filter(Boolean);
    }

    function capacityTargetForScenario(item) {
        const reviewScenarios = state.performanceReview?.scenarios || [];
        const match = reviewScenarios.find((scenario) => (scenario.evidence?.capacity || []).some((row) => {
            return row.id === item.scenario || row.label === item.scenario_label || row.id === item.id;
        }));
        const checks = match?.scale_checks || [];
        const label = `${item.scenario || ""} ${item.scenario_label || ""}`.toLowerCase();
        const desiredUnit = label.includes("tab") ? "tabs"
            : label.includes("view") || label.includes("split") ? "views"
                : label.includes("file count") || label.includes("target-count") || label.includes("workspace") ? "files"
                    : label.includes("byte") || label.includes("size") || label.includes("paste") ? "bytes"
                        : "";
        const check = checks.find((candidate) => candidate.unit === desiredUnit)
            || checks.find((candidate) => candidate.unit !== "ms")
            || checks[0];
        if (check) {
            return {
                value: Number(check.target || 0),
                label: formatScaleValue(check.target, check.unit),
            };
        }
        const fallback = Number(item.first_failure_workload || item.last_successful_workload || 0);
        return {
            value: fallback,
            label: item.first_failure_label || item.last_successful_label || "-",
        };
    }

    function buildResourceIntensityItems() {
        const resourceRows = filterPerformanceScenarioRows(state.resourceProfiles?.scenarios || []);
        const capacityRows = filterPerformanceScenarioRows(state.capacityReport?.scenarios || []);
        const resourceItems = resourceRows.map((item) => {
            const samples = item.samples || [];
            const largest = maxBy(samples, (sample) => Number(sample.workload_value || 0));
            const scale = Number(largest?.workload_value || item.parameter_value || 0);
            const peak = Number(item.max_peak_live_bytes || item.max_working_set_bytes || 0);
            if (!scale || !peak) return null;
            const value = peak / scale;
            const promise = performancePromiseForItem(item);
            return {
                key: `resource:${item.scenario || item.scenario_label}`,
                label: compactScenarioLabel(item),
                detail: `${formatBytes(peak)} / ${largest?.workload_label || formatStressLabel(scale)}`,
                value,
                valueLabel: formatBytes(value),
                family: item.workload_family || "resource",
                resource: item.focus || "resource",
                promiseId: promise.id,
                promiseTitle: promise.title,
                color: promise.color,
                row: item,
            };
        }).filter(Boolean);
        const capacityItems = capacityRows.map((item) => {
            const scale = Number(item.first_failure_workload || item.last_successful_workload || 0);
            const peak = Number(item.peak_working_set_bytes || 0);
            if (!scale || !peak) return null;
            const value = peak / scale;
            const promise = performancePromiseForItem(item);
            return {
                key: `capacity-resource:${item.scenario || item.scenario_label}`,
                label: compactScenarioLabel(item),
                detail: `${formatBytes(peak)} / ${item.first_failure_label || item.last_successful_label || formatStressLabel(scale)}`,
                value,
                valueLabel: formatBytes(value),
                family: item.workload_family || "capacity",
                resource: item.suspected_limiting_resource || "memory",
                profile: (item.matching_flamegraphs || [])[0],
                promiseId: promise.id,
                promiseTitle: promise.title,
                color: promise.color,
                row: item,
            };
        }).filter(Boolean);
        return [...resourceItems, ...capacityItems]
            .filter((item) => Number.isFinite(item.value) && item.value > 0)
            .sort((left, right) => right.value - left.value);
    }

    function buildPressureGrowthPoints(scalingItems) {
        return scalingItems.flatMap((item) => {
            const rows = [...(item.sourceRows || [])]
                .map((row) => ({
                    row,
                    workloadValue: performanceRowParameter(row),
                    workloadLabel: performanceRowParameterLabel(row),
                    pressure: budgetRatio(row),
                    elapsedMs: latencyMs(row),
                    thresholdMs: Number(row.threshold_ms || 0),
                }))
                .filter((point) => point.workloadValue > 0 && point.pressure > 0 && point.elapsedMs > 0)
                .sort((left, right) => left.workloadValue - right.workloadValue);
            if (rows.length < 2) return [];
            const baseline = rows[0];
            const maxLoad = Math.max(...rows.map((point) => point.workloadValue), baseline.workloadValue);
            const baselineLatencyPerUnit = baseline.elapsedMs / baseline.workloadValue;
            return rows.map((current, index) => {
                const loadRatio = current.workloadValue / maxLoad;
                const latencyPerUnit = current.elapsedMs / current.workloadValue;
                const y = latencyPerUnit / baselineLatencyPerUnit;
                const x = loadRatio;
                if (!Number.isFinite(x) || !Number.isFinite(y) || x <= 0 || y <= 0) return null;
                const promise = performancePromiseForItem(current.row);
                return {
                    key: `pressure-growth:${item.key}:point:${index}`,
                    label: `${performanceRowLabel(current.row)} ${current.workloadLabel}`,
                    detail: `${current.workloadLabel} - ${formatMs(current.elapsedMs)} / ${formatMs(current.thresholdMs)} - ${formatRatio(current.pressure)} budget`,
                    x,
                    y,
                    score: x + y,
                    rawXLabel: `${current.workloadLabel} of ${formatStressLabel(maxLoad)}`,
                    rawYLabel: `${formatMs(latencyPerUnit)} per unit vs ${formatMs(baselineLatencyPerUnit)} baseline`,
                    rawXName: "Load point",
                    rawYName: "Latency/unit",
                    profile: (current.row.matching_flamegraphs || [])[0] || item.profile,
                    promiseId: promise.id,
                    promiseTitle: promise.title,
                    color: promise.color,
                    tone: x >= 0.8 && y >= 1 ? "triage" : x >= 0.8 ? "local" : y >= 1 ? "architecture" : "good",
                };
            }).filter(Boolean);
        });
    }

    function buildMemoryElapsedPoints() {
        return filterPerformanceScenarioRows(state.resourceProfiles?.scenarios || [])
            .flatMap((item) => {
                const promise = performancePromiseForItem(item);
                const samples = (item.samples || [])
                    .map((sample, index) => ({
                        sample,
                        index,
                        workloadValue: Number(sample.workload_value || 0),
                        workingSet: Number(sample.working_set_bytes || 0),
                        elapsedMs: Number(sample.elapsed_ms || 0),
                    }))
                    .filter((point) => point.workloadValue > 0 && point.workingSet > 0 && point.elapsedMs > 0)
                    .sort((left, right) => left.workloadValue - right.workloadValue);
                if (samples.length >= 2) {
                    const baseline = samples[0];
                    const maxLoad = Math.max(...samples.map((point) => point.workloadValue), baseline.workloadValue);
                    const baselineMemoryPerUnit = baseline.workingSet / baseline.workloadValue;
                    const baselineElapsedPerUnit = baseline.elapsedMs / baseline.workloadValue;
                    return samples.map((current, index) => {
                        const x = current.workloadValue / maxLoad;
                        const memoryPerUnit = current.workingSet / current.workloadValue;
                        const elapsedPerUnit = current.elapsedMs / current.workloadValue;
                        const memoryCost = memoryPerUnit / baselineMemoryPerUnit;
                        const timeCost = elapsedPerUnit / baselineElapsedPerUnit;
                        const y = Math.max(memoryCost, timeCost);
                        if (!Number.isFinite(x) || !Number.isFinite(y) || x <= 0 || y <= 0) return null;
                        const to = current.sample.workload_label || formatStressLabel(current.workloadValue);
                        const limitingCost = timeCost >= memoryCost ? `time ${formatRatio(timeCost)}` : `memory ${formatRatio(memoryCost)}`;
                        return {
                            key: `memory-elapsed:${item.scenario || item.scenario_label}:point:${index}`,
                            label: `${compactScenarioLabel(item)} ${to}`,
                            detail: `${to} - ${formatMs(current.elapsedMs)} - ${formatBytes(current.workingSet)} working - limiting ${limitingCost}`,
                            x,
                            y,
                            score: x + y,
                            rawXLabel: `${to} of ${formatStressLabel(maxLoad)}`,
                            rawYLabel: `${formatRatio(memoryCost)} memory/unit; ${formatRatio(timeCost)} time/unit`,
                            rawXName: "Load point",
                            rawYName: "Resource/unit",
                            promiseId: promise.id,
                            promiseTitle: promise.title,
                            color: promise.color,
                            tone: x >= 0.8 && y >= 1 ? "triage" : x >= 0.8 ? "local" : y >= 1 ? "architecture" : "good",
                        };
                    })
                        .filter(Boolean);
                }
                const x = Number(item.max_working_set_bytes || 0);
                const y = Number(item.max_elapsed_ms || 0);
                if (!x || !y) return null;
                return [{
                    key: `memory-elapsed:${item.scenario || item.scenario_label}`,
                    label: compactScenarioLabel(item),
                    detail: `${formatBytes(x)} working - ${formatMs(y)}; no comparable load step`,
                    x: 1,
                    y: 1,
                    score: 2,
                    rawXLabel: formatBytes(x),
                    rawYLabel: formatMs(y),
                    rawXName: "Working set",
                    rawYName: "Elapsed",
                    promiseId: promise.id,
                    promiseTitle: promise.title,
                    color: promise.color,
                    tone: "good",
                }];
            })
            .filter(Boolean);
    }

    function buildGrowthScatterBounds(points) {
        const xValues = points.map((point) => point.x).filter((value) => value > 0);
        const yValues = points.map((point) => point.y).filter((value) => value > 0);
        return {
            xMax: Math.max(1, ...xValues, 1) * 1.08,
            yMax: Math.max(2, ...yValues, 1) * 1.08,
        };
    }

    function percentileValue(values, percentile) {
        if (!values.length) return null;
        const index = (values.length - 1) * percentile;
        const lower = Math.floor(index);
        const upper = Math.ceil(index);
        if (lower === upper) return values[lower];
        return values[lower] + (values[upper] - values[lower]) * (index - lower);
    }

    function renderPerformanceDistributionGlyph(options) {
        const items = (options.items || []).filter((item) => Number.isFinite(item.value) && item.value > 0);
        const mode = state.performanceDistributionModes?.[options.id] || "counts";
        const controls = renderPerformanceDistributionControls(options.id, mode);
        if (!items.length) {
            return `<section class="panel-card performance-dist-card">
                <div class="chart-panel__header"><div><h3>${escapeHtml(options.title)}</h3><p class="chart-caption">${escapeHtml(options.caption)}</p></div>${controls}</div>
                <div class="chart-empty">${escapeHtml(options.empty || "No rows loaded.")}</div>
            </section>`;
        }
        const chart = options.shape === "strip"
            ? renderPerformanceStripPlot(items, options)
            : renderPerformanceDistributionCurve(items, options);
        const body = mode === "counts"
            ? renderPerformanceDistributionCounts(items, options)
            : renderPerformanceWorstItems(items, options);
        return `<section class="panel-card performance-dist-card" data-performance-dist="${escapeHtml(options.id)}">
            <div class="chart-panel__header">
                <div>
                    <h3>${escapeHtml(options.title)}</h3>
                    <p class="chart-caption">${escapeHtml(options.caption)}</p>
                </div>
                ${controls}
            </div>
            ${chart}
            ${body}
        </section>`;
    }

    function renderPerformanceDistributionControls(id, mode) {
        return `<div class="segmented-control segmented-control--compact" role="group" aria-label="${escapeHtml(id)} distribution mode">
            ${[["counts", "Counts"], ["worst", "Worst"]].map(([key, label]) => `<button type="button" class="${mode === key ? "is-active" : ""}" aria-pressed="${mode === key ? "true" : "false"}" data-performance-distribution="${escapeHtml(id)}" data-performance-distribution-mode="${key}">${label}</button>`).join("")}
        </div>`;
    }

    function renderPerformanceDistributionCurve(items, options) {
        const values = items.map((item) => item.value).filter((value) => Number.isFinite(value) && value > 0);
        const bounds = normalisePerformanceBounds(values, options.bounds);
        const transform = (value) => performanceScaleTransform(value, bounds);
        const transformed = values.map(transform);
        const total = values.length;
        const meanRaw = mean(values) || 0;
        const meanTransformed = mean(transformed) || 0;
        const variance = transformed.reduce((sum, value) => sum + Math.pow(value - meanTransformed, 2), 0) / Math.max(total, 1);
        const stdDevTransformed = Math.max(Math.sqrt(variance), 0.045);
        const stdDevRaw = Math.sqrt(values.reduce((sum, value) => sum + Math.pow(value - meanRaw, 2), 0) / Math.max(total, 1));
        const width = 640;
        const height = 190;
        const left = 30;
        const right = 610;
        const baseline = 148;
        const top = 24;
        const bucketCount = 22;
        const bins = Array.from({ length: bucketCount }, () => []);
        items.forEach((item) => {
            const index = Math.max(0, Math.min(bucketCount - 1, Math.floor(transform(item.value) * bucketCount)));
            bins[index].push(item);
        });
        const maxBin = Math.max(...bins.map((bin) => bin.length), 1);
        const binPanels = bins.map((bin, index) => renderPerformanceDistributionBinPanel(bin, index, options)).join("");
        const bars = bins.map((bin, index) => {
            const count = bin.length;
            const x = left + (index / bucketCount) * (right - left);
            const barWidth = ((right - left) / bucketCount) - 3;
            const barHeight = (count / maxBin) * 76;
            const low = performanceScaleInverse(index / bucketCount, bounds);
            const high = performanceScaleInverse((index + 1) / bucketCount, bounds);
            const detail = [
                `${formatNumber.format(count)} ${count === 1 ? "item" : "items"}`,
                `${options.valueLabel(low)} to ${options.valueLabel(high)}`,
            ].filter(Boolean).join(" - ");
            const leftPct = ((x + barWidth / 2) / width) * 100;
            const topPct = ((baseline - Math.max(barHeight, 10)) / height) * 100;
            return `<rect x="${x.toFixed(1)}" y="${(baseline - barHeight).toFixed(1)}" width="${barWidth.toFixed(1)}" height="${barHeight.toFixed(1)}" rx="3" class="risk-curve__bar" tabindex="0" role="button" aria-label="${escapeHtml(detail)}" data-performance-bin-index="${index}" data-performance-bin-left="${leftPct.toFixed(2)}" data-performance-bin-top="${topPct.toFixed(2)}"></rect>`;
        }).join("");
        const density = (value) => Math.exp(-0.5 * Math.pow((value - meanTransformed) / stdDevTransformed, 2));
        const maxDensity = Math.max(...Array.from({ length: 80 }, (_, index) => density(index / 79)), 1);
        const points = Array.from({ length: 80 }, (_, index) => {
            const t = index / 79;
            const x = left + t * (right - left);
            const y = baseline - ((density(t) / maxDensity) * (baseline - top));
            return `${x.toFixed(1)},${y.toFixed(1)}`;
        });
        const marker = (entry) => {
            const x = left + transform(entry.value) * (right - left);
            return `<g>
                <line x1="${x.toFixed(1)}" x2="${x.toFixed(1)}" y1="${top}" y2="${baseline}" class="risk-curve__marker risk-curve__marker--${entry.kind}"></line>
                <text class="chart-tick-label performance-dist-marker-label" x="${x.toFixed(1)}" y="${top - 7}" text-anchor="middle">${escapeHtml(entry.label || options.valueLabel(entry.value))}</text>
            </g>`;
        };
        const meanX = left + transform(meanRaw) * (right - left);
        return `<div class="risk-curve-card performance-dist-curve">
            <svg class="risk-curve" viewBox="0 0 ${width} ${height}" role="img" aria-label="${escapeHtml(options.title)} distribution">
                <line x1="${left}" x2="${right}" y1="${baseline}" y2="${baseline}" class="risk-curve__axis"></line>
                ${bars}
                ${(options.markers || []).map(marker).join("")}
                <path d="M ${points.join(" L ")}" class="risk-curve__line"></path>
                <circle cx="${meanX.toFixed(1)}" cy="${baseline - 6}" r="4" class="risk-curve__mean"></circle>
            </svg>
            <div class="performance-bin-popover" hidden></div>
            <div class="performance-bin-panels" hidden>${binPanels}</div>
            <div class="risk-curve-card__stats">
                <span><strong>${formatNumber.format(total)}</strong> items</span>
                <span><strong>${escapeHtml(options.valueLabel(meanRaw))}</strong> mean</span>
                <span><strong>${escapeHtml(options.valueLabel(stdDevRaw))}</strong> std dev</span>
            </div>
        </div>`;
    }

    function renderPerformanceDistributionBinPanel(bin, index, options) {
        const ordered = [...bin].sort((left, right) => right.value - left.value);
        const rows = ordered.map((item) => {
            const color = item.color || performancePromiseColor(item.promiseId || item.label);
            const promise = item.promiseTitle || item.promiseId || "Unmapped";
            return `<div class="performance-bin-row">
                <span class="performance-bin-row__main">
                    <strong title="${escapeHtml(item.label)}">${escapeHtml(shortenLabel(item.label || "Item", 42))}</strong>
                    <em>${escapeHtml([item.detail, item.resource || item.family].filter(Boolean).join(" - "))}</em>
                </span>
                <span class="performance-bin-row__value">${escapeHtml(item.valueLabel || options.valueLabel(item.value))}</span>
                <i class="performance-bin-row__promise" style="--promise-color:${escapeHtml(color)}" title="${escapeHtml(promise)}"></i>
            </div>`;
        }).join("");
        return `<div data-performance-bin-panel="${index}">
            <div class="performance-bin-popover__header">
                <strong>${formatNumber.format(bin.length)} ${bin.length === 1 ? "item" : "items"}</strong>
                <span>${escapeHtml(options.title)}</span>
            </div>
            <div class="performance-bin-popover__list">${rows || `<p>No items in this bucket.</p>`}</div>
        </div>`;
    }

    function renderPerformanceStripPlot(items, options) {
        const sorted = [...items].sort(options.worstSort || ((left, right) => right.value - left.value)).slice(0, 12);
        const bounds = normalisePerformanceBounds(items.map((item) => item.value), options.bounds);
        const x = (value) => performanceScaleTransform(value, bounds) * 100;
        const target = (options.markers || [])[0]?.value ?? 1;
        return `<div class="performance-strip" role="img" aria-label="${escapeHtml(options.title)} strip">
            <div class="performance-strip__target" style="left:${x(target)}%"></div>
            ${sorted.map((item) => {
            const pct = x(item.value);
            const cls = item.failed && item.value < 1 ? "bad" : item.failed ? "watch" : "ok";
            return `<div class="performance-strip-row performance-strip-row--${cls}">
                    <span><strong>${escapeHtml(item.label)}</strong><em>${escapeHtml(item.detail)}</em></span>
                    <div><i style="width:${pct}%"></i><b style="left:${pct}%"></b></div>
                    <strong>${escapeHtml(options.valueLabel(item.value))}</strong>
                </div>`;
        }).join("")}
        </div>`;
    }

    function renderPerformanceDistributionCounts(items, options) {
        const buckets = (options.buckets || []).map((bucket) => ({
            ...bucket,
            value: items.filter(bucket.test).length,
        }));
        const driverCounts = new Map();
        items.forEach((item) => {
            const driver = options.driverFor ? options.driverFor(item) : item.family || item.resource || "other";
            driverCounts.set(driver, (driverCounts.get(driver) || 0) + 1);
        });
        const drivers = [...driverCounts.entries()].sort((left, right) => right[1] - left[1]).slice(0, 6);
        const maxDriver = drivers[0]?.[1] || 1;
        return `<div class="risk-count-grid performance-dist-counts">
            <div class="risk-bucket-list">
                ${buckets.map((bucket) => `<div class="risk-bucket-list__item">
                    <span class="quality-pie__swatch quality-pie__swatch--${bucket.cls}"></span>
                    <span>${escapeHtml(bucket.label)}</span>
                    <strong>${formatNumber.format(bucket.value)}</strong>
                </div>`).join("")}
            </div>
            ${drivers.length ? `<div class="signal-bars">${drivers.map(([driver, count]) => `
                <div class="signal-bars__row" title="${escapeHtml(`${driver}: ${formatNumber.format(count)} ${count === 1 ? "item" : "items"}`)}">
                    <span>${escapeHtml(driver)}</span>
                    <div class="signal-bars__track"><span class="signal-bars__fill" style="width:${(count / maxDriver) * 100}%"></span></div>
                    <span class="signal-bars__count">${formatNumber.format(count)}</span>
                </div>`).join("")}</div>` : `<p class="muted">No driver data.</p>`}
        </div>`;
    }

    function renderPerformanceWorstItems(items, options) {
        const sorted = [...items].sort(options.worstSort || ((left, right) => right.value - left.value)).slice(0, 10);
        return `<div class="quality-feed performance-worst-feed">
            ${sorted.map((item, index) => options.rowFor(item, index, options)).join("")}
        </div>`;
    }

    function renderPerformanceMetricRow(item, index, options) {
        const cls = item.value >= 1 ? "risk-bad" : item.value >= 0.7 ? "risk-warn" : "risk-good";
        return `<div class="quality-feed__row performance-metric-row" style="--promise-color:${escapeHtml(item.color || performancePromiseColor(item.promiseId || item.label))}">
            <span class="rank-pill">${index + 1}</span>
            <span class="quality-feed__name"><code>${escapeHtml(item.label)}</code><span class="muted quality-feed__detail">${escapeHtml([item.promiseTitle, item.detail].filter(Boolean).join(" - "))}</span></span>
            <span class="${cls}">${escapeHtml(item.valueLabel || options.valueLabel(item.value))}</span>
        </div>`;
    }

    function attachPerformanceDistributionHandlers(root) {
        root?.querySelectorAll(".performance-dist-curve").forEach((card) => {
            const popover = card.querySelector(".performance-bin-popover");
            const showPopover = (bar) => {
                if (!bar || !popover) return;
                const index = bar.dataset.performanceBinIndex;
                const panel = card.querySelector(`[data-performance-bin-panel="${CSS.escape(index)}"]`);
                if (!panel) return;
                card.querySelectorAll(".risk-curve__bar").forEach((item) => {
                    item.classList.toggle("is-active", item === bar);
                });
                popover.innerHTML = panel.innerHTML;
                popover.hidden = false;
                const left = Number(bar.dataset.performanceBinLeft || 50);
                const top = Number(bar.dataset.performanceBinTop || 50);
                popover.style.left = `${left}%`;
                popover.style.top = `${top}%`;
                popover.classList.remove(
                    "performance-bin-popover--left",
                    "performance-bin-popover--top",
                    "performance-bin-popover--bottom"
                );
                const cardRect = card.getBoundingClientRect();
                const anchorX = cardRect.left + (cardRect.width * left) / 100;
                const rightEdge = Math.min(window.innerWidth, cardRect.right);
                const leftEdge = Math.max(0, cardRect.left);
                const spaceRight = rightEdge - anchorX;
                const spaceLeft = anchorX - leftEdge;
                if (spaceRight < popover.offsetWidth + 16 && spaceLeft > spaceRight) {
                    popover.classList.add("performance-bin-popover--left");
                }
                if (top < 22) {
                    popover.classList.add("performance-bin-popover--top");
                } else if (top > 70) {
                    popover.classList.add("performance-bin-popover--bottom");
                }
            };
            const hidePopover = () => {
                if (popover) popover.hidden = true;
                card.querySelectorAll(".risk-curve__bar").forEach((item) => item.classList.remove("is-active"));
            };
            card.querySelectorAll(".risk-curve__bar[data-performance-bin-index]").forEach((bar) => {
                bar.addEventListener("click", (event) => {
                    event.stopPropagation();
                    const isOpen = !popover?.hidden && bar.classList.contains("is-active");
                    if (isOpen) {
                        hidePopover();
                    } else {
                        showPopover(bar);
                    }
                });
                bar.addEventListener("keydown", (event) => {
                    if (event.key === "Enter" || event.key === " ") {
                        event.preventDefault();
                        showPopover(bar);
                    }
                    if (event.key === "Escape") {
                        hidePopover();
                        bar.blur();
                    }
                });
            });
            card.addEventListener("click", (event) => {
                const qualityBin = event.target.closest("[data-quality-bin-dataset]");
                if (qualityBin) {
                    event.stopPropagation();
                    drillToQualityDataset(qualityBin.dataset.qualityBinDataset, qualityBin.dataset.qualityBinFilter || "");
                    hidePopover();
                    return;
                }
                if (!event.target.closest(".risk-curve__bar") && !event.target.closest(".performance-bin-popover")) {
                    hidePopover();
                }
            });
        });
    }

    function normalisePerformanceBounds(values, bounds = {}) {
        const finite = values.filter((value) => Number.isFinite(value) && value > 0);
        const scale = bounds.scale || "linear";
        const min = bounds.min ?? (scale === "log" ? Math.max(0.000001, Math.min(...finite, 1) / 1.2) : 0);
        const max = Math.max(bounds.max ?? 0, ...finite, min + 0.0001, 1);
        return { min, max, scale };
    }

    function performanceScaleTransform(value, bounds) {
        if (bounds.scale === "log") {
            const min = Math.max(bounds.min, 0.000001);
            const max = Math.max(bounds.max, min * 1.01);
            return Math.max(0, Math.min(1, (Math.log10(Math.max(value, min)) - Math.log10(min)) / (Math.log10(max) - Math.log10(min))));
        }
        return Math.max(0, Math.min(1, (value - bounds.min) / Math.max(bounds.max - bounds.min, 0.0001)));
    }

    function performanceScaleInverse(value, bounds) {
        const t = Math.max(0, Math.min(1, value));
        if (bounds.scale === "log") {
            const min = Math.max(bounds.min, 0.000001);
            const max = Math.max(bounds.max, min * 1.01);
            return Math.pow(10, Math.log10(min) + t * (Math.log10(max) - Math.log10(min)));
        }
        return bounds.min + t * Math.max(bounds.max - bounds.min, 0.0001);
    }

    function renderPerformanceScatterPanel(options) {
        const rawPoints = (options.points || []).filter((point) => Number.isFinite(point.x) && point.x > 0 && Number.isFinite(point.y) && point.y > 0);
        const normalized = options.normalized === true;
        const normalizeAxisValue = (value, axis) => {
            if (!normalized) return value;
            const threshold = Number(axis.threshold || 0);
            return threshold > 0 ? value / threshold : value;
        };
        const xOptions = normalized ? normalizedScatterAxis(options.x) : options.x;
        const yOptions = normalized ? normalizedScatterAxis(options.y) : options.y;
        const points = rawPoints.map((point) => {
            const xValue = normalizeAxisValue(point.x, options.x);
            const yValue = normalizeAxisValue(point.y, options.y);
            return {
                ...point,
                rawX: point.x,
                rawY: point.y,
                rawXLabel: point.rawXLabel || options.x.valueLabel(point.x),
                rawYLabel: point.rawYLabel || options.y.valueLabel(point.y),
                rawXName: point.rawXName || options.x.rawLabel || options.x.label || "Raw X",
                rawYName: point.rawYName || options.y.rawLabel || options.y.label || "Raw Y",
                x: xValue,
                y: yValue,
                score: normalized ? xValue + yValue : point.score,
            };
        });
        if (!points.length) {
            return `<section class="panel-card ll-plot-card performance-scatter-card">
                <div class="panel-card__header"><div><h3>${escapeHtml(options.title)}</h3><p>${escapeHtml(options.caption)}</p></div></div>
                <div class="chart-empty">${escapeHtml(options.empty || "No points loaded.")}</div>
            </section>`;
        }
        const width = 760;
        const height = 360;
        const margin = { left: 74, right: 28, top: 32, bottom: 58 };
        const plotWidth = width - margin.left - margin.right;
        const plotHeight = height - margin.top - margin.bottom;
        const xBounds = normalisePerformanceBounds(points.map((point) => point.x).concat(xOptions.threshold || []), xOptions);
        const yBounds = normalisePerformanceBounds(points.map((point) => point.y).concat(yOptions.threshold || []), yOptions);
        const x = (value) => margin.left + performanceScaleTransform(value, xBounds) * plotWidth;
        const y = (value) => margin.top + (1 - performanceScaleTransform(value, yBounds)) * plotHeight;
        const xCut = x(xOptions.threshold);
        const yCut = y(yOptions.threshold);
        const ranked = [...points].sort((left, right) => Number(right.score || 0) - Number(left.score || 0)).slice(0, 10);
        const rankedKeys = new Set(ranked.map((point) => point.key));
        const quadrantLabels = (options.quadrants || []).map((quadrant) => {
            const labelX = quadrant.x === "right" ? xCut + 12 : margin.left + 12;
            const labelY = quadrant.y === "top" ? margin.top + 22 : yCut + 22;
            return `<g class="ll-quadrant-label ll-quadrant-label--${escapeHtml(quadrant.tone || "good")}" transform="translate(${labelX.toFixed(1)} ${labelY.toFixed(1)})">
                <text class="ll-quadrant-label__title">${escapeHtml(quadrant.title || "")}</text>
                <text class="ll-quadrant-label__detail" y="16">${escapeHtml(quadrant.detail || "")}</text>
            </g>`;
        }).join("");
        const pointNodes = points.map((point, index) => {
            const radius = Math.max(4, Math.min(11, 3 + Number(point.score || 0)));
            const tone = point.tone || (point.x >= xOptions.threshold && point.y >= yOptions.threshold ? "triage" : point.x >= xOptions.threshold ? "local" : point.y >= yOptions.threshold ? "architecture" : "good");
            const left = (x(point.x) / width) * 100;
            const top = (y(point.y) / height) * 100;
            return `<g class="ll-point is-${tone} ${rankedKeys.has(point.key) ? "is-top-risk" : ""}" tabindex="0" role="button" style="--point-color:${escapeHtml(point.color || performancePromiseColor(point.promiseId || point.label))}" data-scatter-point-index="${index}" data-point-left="${left.toFixed(2)}" data-point-top="${top.toFixed(2)}" data-point-label="${escapeHtml(point.label)}" data-point-detail="${escapeHtml(point.detail || "")}" data-point-x-label="${escapeHtml(xOptions.valueLabel(point.x))}" data-point-y-label="${escapeHtml(yOptions.valueLabel(point.y))}" data-point-x-raw-label="${escapeHtml(point.rawXLabel || "")}" data-point-y-raw-label="${escapeHtml(point.rawYLabel || "")}" data-point-x-name="${escapeHtml(xOptions.label || "X")}" data-point-y-name="${escapeHtml(yOptions.label || "Y")}" data-point-x-raw-name="${escapeHtml(point.rawXName || "Raw X")}" data-point-y-raw-name="${escapeHtml(point.rawYName || "Raw Y")}" data-point-score="${escapeHtml(formatRatio(point.score || 0))}" data-point-profile="${escapeHtml(point.profile || "")}" data-point-promise="${escapeHtml(point.promiseTitle || point.promiseId || "Unmapped")}" aria-label="${escapeHtml(point.label)}">
                <circle cx="${x(point.x).toFixed(1)}" cy="${y(point.y).toFixed(1)}" r="${radius.toFixed(1)}"><title>${escapeHtml(`${point.label}: ${xOptions.valueLabel(point.x)} x ${yOptions.valueLabel(point.y)}`)}</title></circle>
            </g>`;
        }).join("");
        const rankedRows = ranked.map((point, index) => {
            const pointIndex = points.indexOf(point);
            const content = `<span>${index + 1}</span><code>${escapeHtml(shortenLabel(point.label))}</code><strong>${escapeHtml(formatRatio(point.score || 0))}</strong>`;
            return `<button type="button" class="ll-ranked-row" data-scatter-point-index="${pointIndex}">${content}</button>`;
        }).join("");
        return `<section class="panel-card ll-plot-card performance-scatter-card" id="${escapeHtml(options.id)}">
            <div class="panel-card__header">
                <div>
                    <h3>${escapeHtml(options.title)}</h3>
                    <p>${escapeHtml(options.caption)}</p>
                </div>
            </div>
            <div class="ll-plot-layout">
                <svg class="ll-plot" viewBox="0 0 ${width} ${height}" role="img" aria-label="${escapeHtml(options.title)} scatter plot">
                    <rect class="ll-quadrant ll-quadrant--good" x="${margin.left}" y="${yCut}" width="${xCut - margin.left}" height="${margin.top + plotHeight - yCut}"></rect>
                    <rect class="ll-quadrant ll-quadrant--architecture" x="${margin.left}" y="${margin.top}" width="${xCut - margin.left}" height="${yCut - margin.top}"></rect>
                    <rect class="ll-quadrant ll-quadrant--local" x="${xCut}" y="${yCut}" width="${margin.left + plotWidth - xCut}" height="${margin.top + plotHeight - yCut}"></rect>
                    <rect class="ll-quadrant ll-quadrant--triage" x="${xCut}" y="${margin.top}" width="${margin.left + plotWidth - xCut}" height="${yCut - margin.top}"></rect>
                    ${quadrantLabels}
                    <line class="ll-threshold" x1="${xCut}" x2="${xCut}" y1="${margin.top}" y2="${margin.top + plotHeight}"></line>
                    <line class="ll-threshold" x1="${margin.left}" x2="${margin.left + plotWidth}" y1="${yCut}" y2="${yCut}"></line>
                    <line class="ll-axis" x1="${margin.left}" x2="${margin.left}" y1="${margin.top}" y2="${margin.top + plotHeight}"></line>
                    <line class="ll-axis" x1="${margin.left}" x2="${margin.left + plotWidth}" y1="${margin.top + plotHeight}" y2="${margin.top + plotHeight}"></line>
                    <text class="ll-axis-label" x="${margin.left + plotWidth / 2}" y="${height - 16}">${escapeHtml(xOptions.label)}</text>
                    <text class="ll-axis-label ll-axis-label--y" x="20" y="${margin.top + plotHeight / 2}">${escapeHtml(yOptions.label)}</text>
                    <text class="ll-tick" x="${x(xBounds.min)}" y="${height - 38}">${escapeHtml(xOptions.valueLabel(xBounds.min))}</text>
                    <text class="ll-tick" x="${x(xOptions.threshold)}" y="${height - 38}">${escapeHtml(xOptions.valueLabel(xOptions.threshold))}</text>
                    <text class="ll-tick" x="${x(xBounds.max)}" y="${height - 38}">${escapeHtml(xOptions.valueLabel(xBounds.max))}</text>
                    <text class="ll-tick" x="${margin.left - 28}" y="${y(yBounds.max) + 4}">${escapeHtml(yOptions.valueLabel(yBounds.max))}</text>
                    <text class="ll-tick" x="${margin.left - 28}" y="${y(yOptions.threshold) + 4}">${escapeHtml(yOptions.valueLabel(yOptions.threshold))}</text>
                    <text class="ll-tick" x="${margin.left - 28}" y="${y(yBounds.min) + 4}">${escapeHtml(yOptions.valueLabel(yBounds.min))}</text>
                    ${pointNodes}
                </svg>
                <div class="ll-popover" hidden></div>
                <div class="ll-ranked-list">
                    <h3>${escapeHtml(options.sideTitle || "Worst rows")}</h3>
                    ${rankedRows}
                </div>
            </div>
        </section>`;
    }

    function normalizedScatterAxis(axis) {
        const threshold = Number(axis.threshold || 0);
        const normalise = (value) => threshold > 0 ? value / threshold : value;
        return {
            ...axis,
            min: axis.min == null ? axis.min : normalise(axis.min),
            max: axis.max == null ? axis.max : normalise(axis.max),
            threshold: 1,
            scale: "linear",
            valueLabel: formatRatio,
        };
    }

    function attachPerformanceScatterHandlers(root) {
        root?.querySelectorAll(".performance-scatter-card").forEach((card) => {
            const popover = card.querySelector(".ll-popover");
            const setActivePoint = (index) => {
                card.querySelectorAll(".ll-point").forEach((point) => {
                    point.classList.toggle("is-active", Number(point.dataset.scatterPointIndex) === index);
                });
                card.querySelectorAll(".ll-ranked-row").forEach((row) => {
                    row.classList.toggle("is-active", Number(row.dataset.scatterPointIndex) === index);
                });
            };
            const showPopover = (index) => {
                const point = card.querySelector(`.ll-point[data-scatter-point-index="${index}"]`);
                if (!point || !popover) return;
                setActivePoint(index);
                popover.hidden = false;
                const left = Number(point.dataset.pointLeft || 50);
                const top = Number(point.dataset.pointTop || 50);
                popover.classList.toggle("ll-popover--left", left > 68);
                popover.classList.toggle("ll-popover--top", top < 24);
                popover.classList.toggle("ll-popover--bottom", top > 76);
                popover.style.left = `${left}%`;
                popover.style.top = `${top}%`;
                popover.innerHTML = `<strong title="${escapeHtml(point.dataset.pointLabel || "")}">${escapeHtml(point.dataset.pointLabel || "")}</strong>
                    ${point.dataset.pointDetail ? `<p>${escapeHtml(point.dataset.pointDetail)}</p>` : ""}
                    <div><span>Promise</span><b>${escapeHtml(point.dataset.pointPromise || "Unmapped")}</b></div>
                    <div><span>${escapeHtml(point.dataset.pointXName || "X")}</span><b>${escapeHtml(point.dataset.pointXLabel || "-")}</b></div>
                    <div><span>${escapeHtml(point.dataset.pointYName || "Y")}</span><b>${escapeHtml(point.dataset.pointYLabel || "-")}</b></div>
                    ${point.dataset.pointXRawLabel ? `<div><span>${escapeHtml(point.dataset.pointXRawName || "Raw X")}</span><b>${escapeHtml(point.dataset.pointXRawLabel)}</b></div>` : ""}
                    ${point.dataset.pointYRawLabel ? `<div><span>${escapeHtml(point.dataset.pointYRawName || "Raw Y")}</span><b>${escapeHtml(point.dataset.pointYRawLabel)}</b></div>` : ""}
                    <div><span>Score</span><b>${escapeHtml(point.dataset.pointScore || "-")}</b></div>`;
            };
            card.querySelectorAll(".ll-point").forEach((point) => {
                point.addEventListener("click", (event) => {
                    event.stopPropagation();
                    showPopover(Number(point.dataset.scatterPointIndex));
                });
                point.addEventListener("keydown", (event) => {
                    if (event.key === "Enter" || event.key === " ") {
                        event.preventDefault();
                        showPopover(Number(point.dataset.scatterPointIndex));
                    }
                });
            });
            card.querySelectorAll(".ll-ranked-row").forEach((row) => {
                row.addEventListener("click", (event) => {
                    event.stopPropagation();
                    showPopover(Number(row.dataset.scatterPointIndex));
                });
            });
            card.querySelector(".ll-plot-layout")?.addEventListener("click", () => {
                if (popover) popover.hidden = true;
                setActivePoint(-1);
            });
        });
    }

    function renderLatencyWhisker(item) {
        const mean = latencyMs(item);
        const median = item?.median_ns == null ? mean : Number(item.median_ns || 0) / 1_000_000;
        const dispersion = item?.dispersion_ns == null ? 0 : Number(item.dispersion_ns || 0) / 1_000_000;
        const budget = Number(item?.threshold_ms || 0);
        if (!Number.isFinite(mean) || !Number.isFinite(median) || (!mean && !median)) {
            return '<span class="muted">-</span>';
        }
        const start = Math.max(0, median - dispersion);
        const end = median + dispersion;
        const max = Math.max(mean, end, budget, 1);
        const pct = (value) => Math.max(0, Math.min(100, (value / max) * 100));
        return `<div class="latency-whisker" title="median ${escapeHtml(formatMs(median))}, mean ${escapeHtml(formatMs(mean))}, budget ${escapeHtml(formatMs(budget))}">
            <span style="left:${pct(start)}%;width:${Math.max(2, pct(end) - pct(start))}%"></span>
            <i class="latency-whisker__median" style="left:${pct(median)}%"></i>
            <i class="latency-whisker__mean" style="left:${pct(mean)}%"></i>
            ${budget ? `<i class="latency-whisker__budget" style="left:${pct(budget)}%"></i>` : ""}
        </div>`;
    }

    function maxBy(items, valueFn) {
        return (items || []).reduce((best, item) => {
            if (!best) return item;
            return Number(valueFn(item) || 0) > Number(valueFn(best) || 0) ? item : best;
        }, null);
    }

    function formatMs(value) {
        const ms = Number(value || 0);
        if (!Number.isFinite(ms)) return "-";
        return `${formatNumber.format(ms)} ms`;
    }

    function formatRatio(value) {
        const ratio = Number(value || 0);
        if (!Number.isFinite(ratio) || ratio <= 0) return "-";
        if (ratio < 0.01) return "<0.01x";
        return `${formatNumber.format(ratio)}x`;
    }

    function compactScenarioLabel(item) {
        return titleCaseMetricName(item?.scenario_label || item?.scenario || "-")
            .replace(/\bSweep\b/g, "")
            .replace(/\bTracking\b/g, "")
            .replace(/\bProfile\b/g, "")
            .trim();
    }

    function maxMetric(items, field) {
        return (items || []).reduce((max, item) => Math.max(max, Number(item[field] || 0)), 0);
    }

    function renderPerformanceCuratedLists() {
        const target = byId("performance-curated-lists");
        if (!target) return;
        const digest = computePerformanceDigest();
        target.innerHTML = [
            renderRiskList(digest),
            renderHeadroomList(digest),
            renderCoverageGapList(digest),
            renderCapacityList(digest),
        ].join("");
    }

    function renderRiskList(digest) {
        const rows = digest.budgetedRows
            .map((item) => ({ item, ratio: budgetRatio(item) }))
            .filter((row) => row.ratio >= 0.85)
            .sort((a, b) => b.ratio - a.ratio)
            .slice(0, 12);
        return renderCuratedTable({
            title: "Risk List",
            open: true,
            headers: ["Scenario", "Family", "Mean", "Budget", "Ratio", "Resource", "Primary action"],
            rows: rows.map(({ item, ratio }) => `<tr>
                <td><code>${escapeHtml(item.scenario_label || item.name || item.benchmark_key)}</code></td>
                <td><span class="pill">${escapeHtml(item.workload_family || item.family || "unmapped")}</span></td>
                <td>${formatMs(latencyMs(item))}</td>
                <td>${formatMs(item.threshold_ms)}</td>
                <td><span class="status-pill status-pill--${ratio > 1 ? "bad" : "watch"}">${formatRatio(ratio)}</span></td>
                <td><span class="pill">${escapeHtml(item.suspected_limiting_resource || "cpu")}</span></td>
                <td>${renderPrimaryAction(item)}</td>
            </tr>`),
        });
    }

    function renderHeadroomList(digest) {
        const rows = digest.budgetedRows
            .map((item) => ({ item, ratio: budgetRatio(item) }))
            .filter((row) => row.ratio > 0 && row.ratio <= 0.4)
            .sort((a, b) => a.ratio - b.ratio)
            .slice(0, 12);
        return renderCuratedTable({
            title: "Headroom List",
            open: false,
            headers: ["Scenario", "Family", "Mean", "Budget", "Headroom", "Resource"],
            rows: rows.map(({ item, ratio }) => `<tr>
                <td><code>${escapeHtml(item.scenario_label || item.name || item.benchmark_key)}</code></td>
                <td><span class="pill">${escapeHtml(item.workload_family || item.family || "unmapped")}</span></td>
                <td>${formatMs(latencyMs(item))}</td>
                <td>${formatMs(item.threshold_ms)}</td>
                <td><span class="status-pill status-pill--ok">${formatRatio(1 / ratio)}</span></td>
                <td><span class="pill">${escapeHtml(item.suspected_limiting_resource || "cpu")}</span></td>
            </tr>`),
        });
    }

    function renderCoverageGapList(digest) {
        const rows = digest.reviewScenarios
            .map((scenario) => ({ scenario, status: scenarioStatus(scenario), scale: bestScaleCheck(scenario) }))
            .sort((a, b) => statusRank(a.status.cls) - statusRank(b.status.cls) || (b.scenario.budget_misses || 0) - (a.scenario.budget_misses || 0));
        return renderCuratedTable({
            title: "Contract Status List",
            open: false,
            headers: ["Scenario", "Promise", "Scale target", "Observed", "Status"],
            rows: rows.map(({ scenario, status, scale }) => `<tr>
                <td><code>${escapeHtml(scenario.title || scenario.id)}</code></td>
                <td>${escapeHtml(scenario.promise || "-")}</td>
                <td>${scale ? escapeHtml(formatScaleValue(scale.target, scale.unit)) : "-"}</td>
                <td>${scale ? escapeHtml(formatScaleValue(scale.observed, scale.unit)) : "-"}</td>
                <td><span class="status-pill status-pill--${status.cls}">${escapeHtml(status.label)}</span></td>
            </tr>`),
        });
    }

    function renderCapacityList(digest) {
        const rows = digest.capacityRows
            .map((item) => {
                const target = Number(item.first_failure_workload || item.last_successful_workload || 0);
                const ok = Number(item.last_successful_workload || 0);
                const headroom = target ? ok / target : 1;
                return { item, headroom };
            })
            .sort((a, b) => a.headroom - b.headroom);
        return renderCuratedTable({
            title: "Capacity Headroom List",
            open: false,
            headers: ["Scenario", "Last OK", "First failure", "Failure mode", "Resource", "Headroom"],
            rows: rows.map(({ item, headroom }) => `<tr>
                <td><code>${escapeHtml(item.scenario_label || item.scenario)}</code></td>
                <td>${escapeHtml(item.last_successful_label || "-")}</td>
                <td>${escapeHtml(item.first_failure_label || "-")}</td>
                <td><span class="pill">${escapeHtml(item.failure_mode || "not_reached")}</span></td>
                <td><span class="pill">${escapeHtml(item.suspected_limiting_resource || "cpu")}</span></td>
                <td><span class="status-pill status-pill--${headroom < 0.8 ? "watch" : "ok"}">${formatRatio(headroom)}</span></td>
            </tr>`),
        });
    }

    function renderCuratedTable({ title, open, headers, rows }) {
        return `<details class="disclose curated-list" ${open ? "open" : ""}>
            <summary>${escapeHtml(title)}</summary>
            <div class="table-wrap">
                <table>
                    <thead><tr>${headers.map((header) => `<th>${escapeHtml(header)}</th>`).join("")}</tr></thead>
                    <tbody>${rows.length ? rows.join("") : `<tr><td colspan="${headers.length}" class="muted">No rows match.</td></tr>`}</tbody>
                </table>
            </div>
        </details>`;
    }

    function renderPrimaryAction(item) {
        const profiles = item.matching_flamegraphs || [];
        if (profiles.length) {
            return renderPills([profiles[0]]);
        }
        return `<span class="muted">Add profile</span>`;
    }

    function statusRank(cls) {
        return { bad: 0, stale: 1, watch: 2, ok: 3 }[cls] ?? 4;
    }

    function topResourceRows(resources, limit) {
        return (resources || [])
            .map((item) => {
                const peakLive = Number(item.max_peak_live_bytes || 0);
                const workingSet = Number(item.max_working_set_bytes || 0);
                const pageFaults = Number(item.page_fault_count || item.max_page_fault_count || 0);
                return {
                    label: compactScenarioLabel(item),
                    detail: `working ${formatBytes(workingSet)}`,
                    value: peakLive,
                    valueLabel: formatBytes(peakLive),
                    workingSetLabel: formatBytes(workingSet),
                    pageFaultsLabel: pageFaults ? formatNumber.format(pageFaults) : "-",
                };
            })
            .filter((item) => item.value > 0)
            .sort((a, b) => b.value - a.value)
            .slice(0, limit);
    }

    function performanceResultBars(rows, tone) {
        if (!rows.length) {
            return `<p class="performance-result-empty">No result rows loaded.</p>`;
        }
        const max = Math.max(1, ...rows.map((row) => Number(row.value || 0)));
        return `<div class="performance-result-bars performance-result-bars--${tone}">
            ${rows.map((row) => {
            const pct = Math.max(row.value ? 5 : 0, Math.min(100, (Number(row.value || 0) / max) * 100));
            return `<div class="performance-result-bars__row">
                    <span class="performance-result-bars__label">
                        <strong>${escapeHtml(row.label || "-")}</strong>
                        <em>${escapeHtml(row.detail || "")}</em>
                    </span>
                    <div class="performance-result-bars__track"><i style="width:${pct}%"></i></div>
                    <strong class="performance-result-bars__value">${escapeHtml(row.valueLabel || formatNumber.format(row.value || 0))}</strong>
                </div>`;
        }).join("")}
        </div>`;
    }

    function renderChartLegend(series) {
        return series.map((entry) => `<span class="chart-legend__item">
                <svg class="chart-legend__swatch" viewBox="0 0 28 10" aria-hidden="true">
                    <line x1="1" y1="5" x2="27" y2="5" stroke="${entry.color}" stroke-width="3"${entry.dasharray ? ` stroke-dasharray="${entry.dasharray}"` : ""}></line>
                </svg>
                <span>${escapeHtml(entry.label)}</span>
            </span>`).join("");
    }

    function buildLogTicks(min, max) {
        const safeMin = Math.max(min / 1.15, 0.001);
        const safeMax = max * 1.15;
        const ticks = [];
        for (let exponent = Math.floor(Math.log10(safeMin)); exponent <= Math.ceil(Math.log10(safeMax)); exponent += 1) {
            [1, 2, 5].forEach((factor) => {
                const tick = factor * 10 ** exponent;
                if (tick >= safeMin && tick <= safeMax) {
                    ticks.push(tick);
                }
            });
        }
        return ticks.length ? ticks : [safeMin, safeMax];
    }

    function buildLinearTicks(max) {
        const step = max <= 2 ? 0.5 : max <= 4 ? 1 : 2;
        const ticks = [];
        for (let value = 0; value <= max + 0.0001; value += step) {
            ticks.push(Number(value.toFixed(2)));
        }
        return ticks;
    }

    function calculateDoublingMultiplier(points) {
        if (!points || points.length < 2) {
            return null;
        }

        const exponents = [];
        for (let index = 1; index < points.length; index += 1) {
            const previous = points[index - 1];
            const current = points[index];
            const xRatio = current.xValue / previous.xValue;
            const yRatio = current.meanMs / previous.meanMs;
            if (xRatio > 1 && yRatio > 0) {
                exponents.push(Math.log2(yRatio) / Math.log2(xRatio));
            }
        }

        return exponents.length ? 2 ** mean(exponents) : null;
    }

    function mean(values) {
        if (!values.length) {
            return null;
        }
        return values.reduce((sum, value) => sum + value, 0) / values.length;
    }

    function describeGrowth(multiplier) {
        if (multiplier < 1.2) {
            return `nearly flat (${formatNumber.format(multiplier)}x time per 2x growth)`;
        }
        if (multiplier < 1.8) {
            return `sub-linear (${formatNumber.format(multiplier)}x time per 2x growth)`;
        }
        if (multiplier < 2.2) {
            return `roughly linear (${formatNumber.format(multiplier)}x time per 2x growth)`;
        }
        return `super-linear (${formatNumber.format(multiplier)}x time per 2x growth)`;
    }

    function latencyLabel(value) {
        return value === "first_response" ? "First response" : "Completion";
    }

    function titleCase(value) {
        return String(value || "")
            .split(/[_\s-]+/)
            .filter(Boolean)
            .map((item) => item.charAt(0).toUpperCase() + item.slice(1))
            .join(" ");
    }

    function formatAxisMs(value) {
        return value >= 10 ? formatNumber.format(value) : formatNumber.format(Number(value.toFixed(2)));
    }

    function formatBytes(value) {
        if (value == null || !Number.isFinite(value)) {
            return "-";
        }
        if (value >= 1024 * 1024 * 1024) {
            return `${formatNumber.format(value / (1024 * 1024 * 1024))} GB`;
        }
        if (value >= 1024 * 1024) {
            return `${formatNumber.format(value / (1024 * 1024))} MB`;
        }
        if (value >= 1024) {
            return `${formatNumber.format(value / 1024)} KB`;
        }
        return `${formatNumber.format(value)} B`;
    }

    function pillValues(value) {
        return Array.isArray(value)
            ? value
            : String(value || "")
                .split(",")
                .map((item) => item.trim())
                .filter(Boolean);
    }

    function renderPills(value) {
        const values = pillValues(value);
        if (!values.length) {
            return '<span class="muted">-</span>';
        }
        return values.map((item) => `<span class="pill">${escapeHtml(item)}</span>`).join("");
    }

    function renderOverview() {
        renderHealthGauges();
        renderRiskTreemap();
        renderProjectCodeMetrics();
        renderTopConcerns();
        renderRunStrip();
    }

    function classifyStatus(level) {
        if (level === "stale") return { label: "Stale", cls: "stale" };
        if (level === "bad") return { label: "Regressed", cls: "bad" };
        if (level === "watch") return { label: "Watch", cls: "watch" };
        return { label: "OK", cls: "ok" };
    }

    function renderGaugeCard({ id, title, value, status, driver, sparkline, deltaInfo }) {
        const s = classifyStatus(status);
        const delta = deltaInfo
            ? `<span class="gauge-card__delta gauge-card__delta--${deltaInfo.direction}">${escapeHtml(deltaInfo.label)}</span>`
            : `<span class="gauge-card__delta">no history</span>`;
        const spark = sparkline && sparkline.length >= 2 ? renderSparkline(sparkline) : "";
        return `<div class="gauge-card gauge-card--${s.cls}" id="${id}">
            <div class="gauge-card__title">
                <span>${escapeHtml(title)}</span>
                <span class="gauge-card__status gauge-card__status--${s.cls}">${s.label}</span>
            </div>
            <div class="gauge-card__metric">
                <span class="gauge-card__value">${escapeHtml(value)}</span>
                ${delta}
            </div>
            <div class="gauge-card__driver">${escapeHtml(driver || "")}</div>
            ${spark}
        </div>`;
    }

    function renderSparkline(values) {
        if (!values.length) return "";
        const w = 200, h = 44, pad = 2;
        const min = Math.min(...values);
        const max = Math.max(...values);
        const range = max - min || 1;
        const points = values.map((v, i) => {
            const x = pad + (i * (w - pad * 2)) / Math.max(1, values.length - 1);
            const y = h - pad - ((v - min) / range) * (h - pad * 2);
            return `${x.toFixed(1)},${y.toFixed(1)}`;
        }).join(" ");
        const last = values[values.length - 1];
        const lastX = w - pad;
        const lastY = h - pad - ((last - min) / range) * (h - pad * 2);
        return `<svg class="gauge-card__sparkline" viewBox="0 0 ${w} ${h}" preserveAspectRatio="none">
            <polyline fill="none" stroke="#6fd0ff" stroke-width="2" points="${points}" />
            <circle cx="${lastX}" cy="${lastY.toFixed(1)}" r="2.5" fill="#6fd0ff" />
        </svg>`;
    }

    function runMetricSeries(metricKey) {
        // Pull headline metric values from finished runs in chronological order.
        return state.runs
            .filter((run) => run.metrics && run.metrics[metricKey] != null && run.finished_at)
            .map((run) => Number(run.metrics[metricKey]))
            .filter((v) => Number.isFinite(v))
            .slice(-12);
    }

    function describeDelta(series, { higherIsBetter = false } = {}) {
        if (!series || series.length < 2) return null;
        const last = series[series.length - 1];
        const prev = series[series.length - 2];
        if (prev === 0 && last === 0) return null;
        const diff = last - prev;
        const pct = prev === 0 ? null : (diff / Math.abs(prev)) * 100;
        const arrow = diff === 0 ? "" : diff > 0 ? "▲" : "▼";
        const direction = diff === 0 ? "flat" : (diff > 0 ? (higherIsBetter ? "down" : "up") : (higherIsBetter ? "up" : "down"));
        const label = pct == null ? `${arrow} ${formatNumber.format(diff)}` : `${arrow} ${formatNumber.format(Math.abs(pct))}%`;
        return { direction, label };
    }

    function computeQualityHealth() {
        const hotspots = state.hotspots || [];
        const clones = state.clones || [];
        const bad = hotspots.filter((h) => qualityScore(h) >= 600).length;
        const warn = hotspots.filter((h) => {
            const s = qualityScore(h);
            return s >= 300 && s < 600;
        }).length;
        const cloneRisks = clones.filter((c) => (c.score || 0) >= 40).length;
        const total = bad + warn + cloneRisks;
        let status = "ok";
        if (bad > 0 || cloneRisks > 0) status = "bad";
        else if (warn > 0) status = "watch";
        if (!hotspots.length && !clones.length) status = "stale";
        const worst = hotspots[0];
        const driver = worst
            ? `Worst: ${worst.name.split(/[\\/]/).pop()} (${formatNumber.format(qualityScore(worst))})`
            : "No hotspots data";
        return {
            status,
            value: String(total),
            driver,
            series: runMetricSeries("quality_risk_count"),
        };
    }

    function computeCapacityHealth() {
        const speed = state.speedReport || {};
        const summary = speed.summary || {};
        const reviewSummary = state.performanceReview?.summary || {};
        const triageSummary = speed.triage_summary || null;
        const overBudget = summary.over_budget_latency ?? 0;
        const implementations = reviewSummary.implementation_count ?? 0;
        const ceilings = summary.near_failure_ceilings ?? 0;
        let status = "ok";
        let value = "0";
        let driver = "All scenarios within budget";
        if (triageSummary) {
            const critical = triageSummary.critical ?? 0;
            const watch = triageSummary.watch ?? 0;
            value = String(critical + watch);
            if (critical > 0) { status = "bad"; driver = `${critical} critical, ${watch} to watch`; }
            else if (watch > 0) { status = "watch"; driver = `${watch} scenarios approaching budget`; }
        } else {
            const total = overBudget + ceilings;
            value = String(total);
            if (overBudget > 0 || ceilings > 0) {
                status = overBudget > 2 || ceilings > 0 ? "bad" : "watch";
                driver = `${overBudget} over budget, ${ceilings} near ceiling, ${implementations} measurements`;
            }
        }
        if (!state.speedReport) { status = "stale"; value = "—"; driver = "Run performance refresh"; }
        return {
            status,
            value,
            driver,
            series: runMetricSeries("capacity_risk_count"),
        };
    }

    function speedReportScenarios() {
        const sections = state.speedReport?.sections;
        if (!sections || typeof sections !== "object") {
            return [];
        }
        return Object.values(sections).flatMap((items) => Array.isArray(items) ? items : []);
    }

    function currentFrameScenario() {
        const scenarios = speedReportScenarios();
        const preferredIds = [
            "editor_scroll_frame_120hz/4194304",
            "editor_scroll_frame_120hz/1048576",
            "ui_render_frame_120hz",
            "ui_render_frame",
        ];
        return preferredIds
            .map((id) => scenarios.find((item) =>
                item?.scenario_id === id
                || item?.scenario_label === id
                || item?.benchmark_key === id
            ))
            .find(Boolean);
    }

    function computeFpsHealth() {
        const scenario = currentFrameScenario();
        const scenarioId = scenario?.scenario_id || scenario?.benchmark_key || scenario?.scenario_label || "";
        const frameMs = Number(scenario?.p99_ms ?? scenario?.p95_ms ?? scenario?.mean_ms);
        const goalFps = scenarioId.includes("120hz") ? 120 : 60;
        const goalFrameMs = 1000 / goalFps;
        if (!scenario || !Number.isFinite(frameMs) || frameMs <= 0) {
            return {
                status: "stale",
                value: "—",
                driver: "Run performance refresh",
                series: runMetricSeries("app_fps"),
            };
        }

        const fps = 1000 / frameMs;
        const status = frameMs <= goalFrameMs && !scenario.over_budget ? "ok" : "bad";
        const frameLabel = scenario?.p99_ms ? "p99" : scenario?.p95_ms ? "p95" : "avg";
        const loadLabel = scenarioId.includes("4194304")
            ? "4 MiB wheel scroll"
            : scenarioId.includes("1048576")
                ? "1 MiB wheel scroll"
                : "steady repaint";
        const supporting = [
            scenario?.p95_ms ? `p95 ${formatNumber.format(scenario.p95_ms)} ms` : null,
            scenario?.mean_ms ? `mean ${formatNumber.format(scenario.mean_ms)} ms` : null,
        ].filter(Boolean).join(", ");
        return {
            status,
            value: `${formatNumber.format(fps)} FPS`,
            driver: `${frameLabel} ${formatNumber.format(frameMs)} ms, ${loadLabel}, ${goalFps} FPS goal (${formatNumber.format(goalFrameMs)} ms)${supporting ? `; ${supporting}` : ""}`,
            series: runMetricSeries("app_fps"),
        };
    }

    function computeCorrectnessHealth() {
        const c = state.correctness || {};
        const summary = c.summary || {};
        const failed = summary.failed ?? 0;
        const unknown = summary.unknown ?? 0;
        const total = summary.test_count ?? 0;
        const lastRun = correctnessLastRun(summary);
        const runFailed = correctnessRunFailed(summary);
        let status = "ok";
        let driver = `${total} tests, all passing`;
        if (runFailed) { status = "bad"; driver = compactCorrectnessRunMessage(lastRun); }
        else if (failed > 0) { status = "bad"; driver = `${failed} failed, ${unknown} unknown`; }
        else if (unknown > 0) { status = "watch"; driver = `${unknown} tests have not been run`; }
        if (!state.correctness) { status = "stale"; driver = "Run correctness refresh"; }
        const value = state.correctness ? (runFailed ? "run failed" : `${total - failed - unknown}/${total}`) : "—";
        return {
            status,
            value,
            driver,
            series: runMetricSeries("tests_passed"),
        };
    }

    function correctnessLastRun(summary) {
        const lastRun = summary?.last_run;
        return lastRun && typeof lastRun === "object" ? lastRun : null;
    }

    function correctnessRunFailed(summary) {
        return correctnessLastRun(summary)?.status === "failed";
    }

    function compactCorrectnessRunMessage(lastRun) {
        const raw = [lastRun?.stderr_tail, lastRun?.stdout_tail]
            .filter(Boolean)
            .join("\n")
            .trim();
        if (!raw) return "cargo test did not complete";
        const lines = raw.split(/\r?\n/).map((line) => line.trim()).filter(Boolean);
        const errorLine = lines.find((line) => line.startsWith("error:"));
        const accessLine = lines.find((line) => /access is denied|os error/i.test(line));
        if (errorLine && accessLine && errorLine !== accessLine) {
            return `${errorLine}; ${accessLine}`;
        }
        return errorLine || accessLine || lines[lines.length - 1] || "cargo test failed";
    }

    function renderHealthGauges() {
        const target = byId("overview-health");
        if (!target) return;
        const quality = computeQualityHealth();
        const capacity = computeCapacityHealth();
        const fps = computeFpsHealth();
        const correctness = computeCorrectnessHealth();
        target.innerHTML = [
            renderGaugeCard({
                id: "gauge-quality",
                title: "Quality",
                value: quality.value,
                status: quality.status,
                driver: quality.driver,
                sparkline: quality.series,
                deltaInfo: describeDelta(quality.series),
            }),
            renderGaugeCard({
                id: "gauge-capacity",
                title: "Capacity",
                value: capacity.value,
                status: capacity.status,
                driver: capacity.driver,
                sparkline: capacity.series,
                deltaInfo: describeDelta(capacity.series),
            }),
            renderGaugeCard({
                id: "gauge-fps",
                title: "FPS",
                value: fps.value,
                status: fps.status,
                driver: fps.driver,
                sparkline: fps.series,
                deltaInfo: describeDelta(fps.series, { higherIsBetter: true }),
            }),
            renderGaugeCard({
                id: "gauge-correctness",
                title: "Correctness",
                value: correctness.value,
                status: correctness.status,
                driver: correctness.driver,
                sparkline: correctness.series,
                deltaInfo: describeDelta(correctness.series, { higherIsBetter: true }),
            }),
        ].join("");
    }

    function moduleScoreFor(module, metric) {
        const m = module.metrics || {};
        if (metric === "total_score") return Number(module.total_score ?? m.total_score ?? 0);
        return Number(module[metric] ?? m[metric] ?? 0);
    }

    function moduleSloc(module) {
        const m = module.metrics || {};
        return Number(module.sloc ?? module.size ?? m.sloc ?? m.size ?? 1);
    }

    function moduleSignals(module) {
        const m = module.metrics || {};
        if (Array.isArray(module.signals)) return module.signals;
        if (Array.isArray(m.signals)) return m.signals;
        return [];
    }

    function moduleRiskDrivers(module) {
        return riskMetricLabels
            .map(([key, label]) => ({ label, score: moduleScoreFor(module, key) }))
            .filter((item) => item.score > 0)
            .sort((a, b) => b.score - a.score)
            .slice(0, 3);
    }

    function riskTooltip(module, score) {
        const signals = moduleSignals(module).slice(0, 3);
        const drivers = moduleRiskDrivers(module)
            .map((item) => `${item.label} ${formatNumber.format(item.score)}`);
        return [
            module.id || module.name || "Unknown module",
            `Total risk ${formatNumber.format(score)}`,
            drivers.length ? `Top drivers: ${drivers.join(", ")}` : "",
            signals.length ? `Signals: ${signals.join("; ")}` : "",
            `SLOC ${formatNumber.format(moduleSloc(module))}`,
        ].filter(Boolean).join("\n");
    }

    function colorForScore(score, max) {
        const t = max ? Math.min(1, score / max) : 0;
        // Interpolate good -> warn -> bad.
        if (t < 0.5) {
            const k = t / 0.5;
            return mixColor([125, 220, 155], [243, 201, 105], k);
        }
        const k = (t - 0.5) / 0.5;
        return mixColor([243, 201, 105], [255, 116, 116], k);
    }

    function mixColor(a, b, t) {
        const r = Math.round(a[0] + (b[0] - a[0]) * t);
        const g = Math.round(a[1] + (b[1] - a[1]) * t);
        const bl = Math.round(a[2] + (b[2] - a[2]) * t);
        return `rgb(${r}, ${g}, ${bl})`;
    }

    function renderRiskTreemap() {
        const target = byId("overview-treemap");
        if (!target) return;
        const graph = state.map?.graph;
        if (!graph || !graph.nodes) {
            target.innerHTML = `<div class="risk-treemap__empty">No map data. Run the Map refresh.</div>`;
            return;
        }
        const modules = graph.nodes
            .map((n) => n.data)
            .filter((n) => n && !n.is_group);
        if (!modules.length) {
            target.innerHTML = `<div class="risk-treemap__empty">No modules to display.</div>`;
            return;
        }
        const filtered = modules
            .map((m) => ({ module: m, score: moduleScoreFor(m, "total_score") }))
            .filter(({ module, score }) => {
                if (state.overviewRiskFilter === "high") return score >= 600;
                if (state.overviewRiskFilter === "with-signals") return moduleSignals(module).length > 0;
                return true;
            })
            .sort((a, b) => b.score - a.score);
        const ranked = state.overviewRiskMode === "top" ? filtered.slice(0, 24) : filtered;
        if (!ranked.length) {
            target.innerHTML = `<div class="risk-treemap__empty">No modules match the selected risk filters.</div>`;
            return;
        }
        const maxScore = ranked[0]?.score || 1;
        const scopeLabel = state.overviewRiskMode === "top" ? "Top risk" : "All modules";
        const filterLabel = state.overviewRiskFilter === "high" ? "high risk only"
            : state.overviewRiskFilter === "with-signals" ? "with signals"
                : "all risk levels";
        target.innerHTML = `<div class="risk-treemap__meta">${escapeHtml(scopeLabel)}: showing ${ranked.length} of ${modules.length} modules, ${escapeHtml(filterLabel)}. Highest score ${formatNumber.format(maxScore)}.</div>` + ranked.map(({ module, score }) => {
            const sloc = moduleSloc(module);
            const widthBasis = Math.max(80, Math.min(260, 60 + Math.sqrt(sloc) * 4));
            const color = colorForScore(score, maxScore);
            const label = (module.name || module.id || "?").split(/[\\/]/).slice(-2).join("/");
            const drivers = moduleRiskDrivers(module).map((item) => item.label).join(", ");
            return `<div class="risk-treemap__cell" style="flex: 1 1 ${widthBasis}px; background:${color};" title="${escapeHtml(riskTooltip(module, score))}" data-module-id="${escapeHtml(module.id)}">
                <span class="risk-treemap__label">${escapeHtml(label)}</span>
                <span class="risk-treemap__score">${formatNumber.format(score)}</span>
                ${drivers ? `<span class="risk-treemap__drivers">${escapeHtml(drivers)}</span>` : ""}
            </div>`;
        }).join("");
        target.querySelectorAll(".risk-treemap__cell").forEach((el) => {
            el.addEventListener("click", () => {
                state.selectedModule = el.dataset.moduleId;
                document.querySelector('.tab[data-tab="map"]')?.click();
                renderMap();
            });
        });
    }

    function codeMetricParts() {
        const current = state.projectCodeMetrics?.current || {};
        return [
            { key: "application", label: "Application", value: Number(current.application || 0), color: "#6fd0ff" },
            { key: "test", label: "Test", value: Number(current.test || 0), color: "#7ddc9b" },
            { key: "other", label: "Other", value: Number(current.other || 0), color: "#f3c969" },
        ];
    }

    function renderProjectCodeMetrics() {
        renderCodePie();
        renderCodeHistory();
    }

    function renderCodePie() {
        const target = byId("overview-code-pie");
        if (!target) return;
        if (!state.projectCodeMetrics) {
            target.innerHTML = `<p class="muted">No project code metrics loaded. Refresh Project Code Metrics.</p>`;
            return;
        }
        const parts = codeMetricParts();
        const total = parts.reduce((sum, item) => sum + item.value, 0);
        const latest = state.projectCodeMetrics.latest_push || {};
        let offset = 0;
        const stops = parts.map((item) => {
            const start = offset;
            const span = total ? (item.value / total) * 100 : 0;
            offset += span;
            return `${item.color} ${start}% ${offset}%`;
        }).join(", ");
        const latestDate = latest.date ? new Date(latest.date).toLocaleDateString() : "-";
        target.innerHTML = `<div class="code-pie__chart" style="background: conic-gradient(from -90deg, ${stops || "#344151 0 100%"})">
                <div class="code-pie__center">
                    <span>Total Rust</span>
                    <strong>${formatNumber.format(total)}</strong>
                </div>
            </div>
            <div class="code-pie__content">
                <div class="code-pie__latest">
                    <span>Latest GitHub Push</span>
                    <strong>${escapeHtml(latest.short_sha || "-")}</strong>
                    <p>${escapeHtml(latestDate)} · ${escapeHtml(latest.subject || "-")}</p>
                </div>
                <div class="code-pie__legend">
                    ${parts.map((item) => `<div class="code-pie__legend-row">
                        <span><i style="background:${item.color}"></i>${escapeHtml(item.label)}</span>
                        <strong>${formatNumber.format(item.value)}</strong>
                    </div>`).join("")}
                </div>
            </div>`;
    }

    function renderCodeHistory() {
        const target = byId("overview-code-history");
        if (!target) return;
        const history = state.projectCodeMetrics?.history || [];
        if (history.length < 2) {
            target.innerHTML = `<p class="muted">No GitHub line history loaded yet.</p>`;
            return;
        }
        const w = 720, h = 260, padLeft = 58, padRight = 18, padTop = 18, padBottom = 42;
        const series = codeMetricParts().map((part) => ({
            ...part,
            values: history.map((item) => Number(item.lines?.[part.key] || 0)),
        }));
        const allValues = series.flatMap((item) => item.values);
        const min = Math.min(...allValues);
        const max = Math.max(...allValues);
        const range = max - min || 1;
        const xFor = (index) => padLeft + (index * (w - padLeft - padRight)) / Math.max(1, history.length - 1);
        const yFor = (value) => h - padBottom - ((value - min) / range) * (h - padTop - padBottom);
        const lineSeries = series.map((item) => {
            const points = item.values.map((value, index) => `${xFor(index).toFixed(1)},${yFor(value).toFixed(1)}`).join(" ");
            return `<polyline class="code-history__line" style="--series-color:${item.color}" points="${points}" />`;
        }).join("");
        const first = history[0];
        const last = history[history.length - 1];
        const latestApplication = Number(last.lines?.application || state.projectCodeMetrics?.current?.application || 0);
        const ticks = [min, min + range / 2, max].map((value) => {
            const y = yFor(value);
            return `<g>
                <line x1="${padLeft}" x2="${w - padRight}" y1="${y.toFixed(1)}" y2="${y.toFixed(1)}" />
                <text x="${padLeft - 10}" y="${(y + 4).toFixed(1)}">${escapeHtml(formatNumber.format(Math.round(value)))}</text>
            </g>`;
        }).join("");
        const markers = series.map((seriesItem) => history.map((item, index) => {
            const date = item.date ? new Date(item.date).toLocaleDateString() : "-";
            const value = seriesItem.values[index];
            const title = `${date}: ${seriesItem.label} ${formatNumber.format(value)} lines\n${item.short_sha || ""} ${item.subject || ""}`;
            return `<circle style="--series-color:${seriesItem.color}" cx="${xFor(index).toFixed(1)}" cy="${yFor(value).toFixed(1)}" r="2.7"><title>${escapeHtml(title)}</title></circle>`;
        }).join("")).join("");
        const legend = series.map((item) => {
            const latest = item.values[item.values.length - 1] || 0;
            return `<span class="code-history__legend-item">
                <i style="background:${item.color}"></i>
                ${escapeHtml(item.label)}
                <strong>${formatNumber.format(latest)}</strong>
            </span>`;
        }).join("");
        target.innerHTML = `<div class="code-history__meta">
                <span>${escapeHtml(first.date ? new Date(first.date).toLocaleDateString() : "-")}</span>
                <strong>Application Code: ${formatNumber.format(latestApplication)}</strong>
                <span>${escapeHtml(last.date ? new Date(last.date).toLocaleDateString() : "-")}</span>
            </div>
            <svg class="code-history__chart" viewBox="0 0 ${w} ${h}" role="img" aria-label="Application, test, and other Rust code lines over time">
                <g class="code-history__grid">${ticks}</g>
                <g class="code-history__series">${lineSeries}</g>
                <g class="code-history__markers">${markers}</g>
            </svg>
            <div class="code-history__legend">${legend}</div>`;
    }

    function renderTopListCard({ title, subtitle, items, emptyText, tone = "neutral" }) {
        const list = items.length
            ? `<ol>${items.map((item, index) => {
                const labelClass = `top-list-card__label${item.preserveEnd ? " top-list-card__label--end" : ""}`;
                const titleAttr = item.tooltip ? ` title="${escapeHtml(item.tooltip)}"` : "";
                return `<li><span class="top-list-card__rank">${index + 1}</span><span class="${labelClass}"${titleAttr}>${item.label}</span><span class="top-list-card__value">${item.value}</span></li>`;
            }).join("")}</ol>`
            : `<p class="muted">${escapeHtml(emptyText)}</p>`;
        return `<div class="top-list-card top-list-card--${escapeHtml(tone)}">
            <div class="top-list-card__header">
                <span class="top-list-card__marker"></span>
                <div>
                    <h3>${escapeHtml(title)}</h3>
                    <p>${escapeHtml(subtitle)}</p>
                </div>
            </div>
            ${list}
        </div>`;
    }

    function renderTopConcerns() {
        const target = byId("overview-top-concerns");
        if (!target) return;

        const qualityItems = [...(state.hotspots || [])]
            .sort((a, b) => qualityScore(b) - qualityScore(a))
            .slice(0, 5)
            .map((item) => ({
                label: `<code>${escapeHtml((item.name || "").split(/[\\/]/).pop() || item.name)}</code>`,
                value: `<span class="${riskClass(qualityScore(item), 300, 600)}">${formatNumber.format(qualityScore(item))}</span>`,
            }));

        const slowItems = distinctPerformanceRows([...(state.slowspots || []), ...(state.searchSpeed || [])])
            .map((item) => ({
                name: performanceRowLabel(item),
                ratio: item.threshold_ms ? (item.mean_ns / 1_000_000) / item.threshold_ms : 0,
            }))
            .filter((it) => it.ratio > 0)
            .sort((a, b) => b.ratio - a.ratio)
            .slice(0, 5)
            .map((it) => ({
                label: `<code>${escapeHtml(it.name)}</code>`,
                tooltip: it.name,
                preserveEnd: true,
                value: `<span class="${it.ratio > 1 ? "risk-bad" : it.ratio > 0.85 ? "risk-warn" : "risk-good"}">${formatNumber.format(it.ratio * 100)}%</span>`,
            }));

        const tests = state.correctness?.tests || [];
        const lastRun = state.correctness?.summary?.last_run;
        const testItems = tests
            .filter((t) => t.last_status === "failed" || t.last_status === "unknown")
            .slice(0, 5)
            .map((t) => ({
                label: `<code>${escapeHtml(t.name || t.path)}</code>`,
                value: `<span class="${t.last_status === "failed" ? "risk-bad" : "risk-warn"}">${escapeHtml(t.last_status)}</span>`,
            }));
        if (lastRun?.status === "failed") {
            testItems.unshift({
                label: `<code>cargo test run</code>`,
                value: `<span class="risk-bad">failed</span>`,
            });
            testItems.splice(5);
        }

        const diagnosticItems = commonDiagnosticItems();

        target.innerHTML = [
            renderTopListCard({
                title: "Top quality risks",
                subtitle: "Highest hotspot scores.",
                items: qualityItems,
                emptyText: "No hotspot data.",
                tone: "quality",
            }),
            renderTopListCard({
                title: "Slowest vs budget",
                subtitle: "Mean latency relative to threshold.",
                items: slowItems,
                emptyText: "No benchmark data.",
                tone: "speed",
            }),
            renderTopListCard({
                title: "Failing or unknown tests",
                subtitle: "Need attention or have not run.",
                items: testItems,
                emptyText: state.correctness ? "All tests passing." : "No test data.",
                tone: "correctness",
            }),
            renderTopListCard({
                title: "Top diagnostics",
                subtitle: "Most common app package messages.",
                items: diagnosticItems,
                emptyText: state.appPackage ? "No diagnostics recorded." : "No app package data.",
                tone: "diagnostics",
            }),
        ].join("");
    }

    function commonDiagnosticItems() {
        const diagnostics = state.appPackage?.diagnostics || [];
        const grouped = new Map();
        diagnostics.forEach((item, index) => {
            const message = String(item.message || item.operation || item.kind || "Unknown diagnostic").trim();
            const key = message.toLowerCase();
            const existing = grouped.get(key) || { message, count: 0, lastIndex: index };
            existing.count += 1;
            existing.lastIndex = index;
            grouped.set(key, existing);
        });
        return [...grouped.values()]
            .sort((a, b) => b.count - a.count || b.lastIndex - a.lastIndex)
            .slice(0, 5)
            .map((item) => ({
                label: `<code>${escapeHtml(item.message)}</code>`,
                value: `<span class="${item.count > 1 ? "risk-warn" : "risk-good"}">${formatNumber.format(item.count)}</span>`,
            }));
    }

    function renderRunStrip() {
        const target = byId("overview-run-strip");
        if (!target) return;
        const runs = [...state.runs].slice(-12).reverse();
        if (!runs.length) {
            target.innerHTML = `<p class="muted">No runs yet. Use the Refresh buttons above to start one.</p>`;
            return;
        }
        target.innerHTML = runs.map((run) => {
            const status = (run.status || "queued").toLowerCase();
            const dur = run.duration_seconds == null ? "" : ` ${formatNumber.format(run.duration_seconds)}s`;
            const ts = run.finished_at || run.started_at || run.created_at || 0;
            const tsLabel = ts ? new Date(ts * 1000).toLocaleTimeString() : run.id;
            const isActive = state.selectedRun === run.id;
            return `<button type="button" class="run-strip__dot ${isActive ? "is-active" : ""}" data-run-id="${escapeHtml(run.id)}" title="${escapeHtml(run.selector || run.id)}">
                <span class="run-strip__bullet run-strip__bullet--${status}"></span>
                <span>${escapeHtml(tsLabel)}${escapeHtml(dur)}</span>
            </button>`;
        }).join("");
        target.querySelectorAll(".run-strip__dot").forEach((el) => {
            el.addEventListener("click", () => {
                const id = el.dataset.runId;
                state.selectedRun = id;
                const out = byId("overview-run-log");
                if (out) {
                    out.hidden = false;
                    out.textContent = "Loading run log...";
                }
                loadRunLog(id, "overview-run-log");
                renderRunStrip();
            });
        });
    }

    function renderCorrectness() {
        const payload = state.correctness || {};
        const summary = payload.summary || {};
        const layers = payload.layers || [];
        const tests = payload.tests || [];
        const query = byId("correctness-filter")?.value || "";
        const showAll = byId("correctness-show-all")?.checked ?? false;
        const layerFilter = state.selectedLayer;
        const categoryFilter = state.selectedCorrectnessCategory;
        let filtered = tests.filter((item) => matchesFilter(item, query));
        if (layerFilter) {
            filtered = filtered.filter((item) => item.layer === layerFilter);
        }
        if (categoryFilter) {
            filtered = filtered.filter((item) => inlineTestCategory(item) === categoryFilter);
        }
        if (!showAll) {
            filtered = filtered.filter((item) => item.last_status === "failed" || item.last_status === "unknown");
        }
        renderCorrectnessOverview(payload, filtered, { query, showAll, layerFilter, categoryFilter });
        renderCorrectnessSummary(payload);
        renderTable(
            "correctness-layers",
            ["Layer", "Total", "Passed", "Failed", "Skipped", "Unknown"],
            layers.map((item) => `<tr>
                <td>${escapeHtml(item.name)}</td>
                <td>${formatNumber.format(item.total || 0)}</td>
                <td class="risk-good">${formatNumber.format(item.passed || 0)}</td>
                <td class="${item.failed ? "risk-bad" : "risk-good"}">${formatNumber.format(item.failed || 0)}</td>
                <td>${formatNumber.format(item.skipped || 0)}</td>
                <td>${formatNumber.format(item.unknown || 0)}</td>
            </tr>`)
        );
        renderTable(
            "correctness-table",
            ["Layer", "Module", "Test", "Description", "Category", "Status", "Command"],
            filtered.map((item) => `<tr>
                <td><span class="pill">${escapeHtml(item.layer)}</span></td>
                <td><code>${escapeHtml(item.module || "-")}</code></td>
                <td><code>${escapeHtml(item.path)}:${escapeHtml(item.line)}</code><div class="muted">${escapeHtml(item.name)}</div></td>
                <td>${escapeHtml(item.description)}</td>
                <td><button type="button" class="pill correctness-category-pill" data-correctness-category="${escapeHtml(inlineTestCategory(item))}">${escapeHtml(inlineTestCategory(item))}</button></td>
                <td class="${item.last_status === "failed" ? "risk-bad" : item.last_status === "passed" ? "risk-good" : "risk-warn"}">${escapeHtml(item.last_status)}</td>
                <td><code>${escapeHtml(item.command)}</code></td>
            </tr>`)
        );
        attachCorrectnessCategoryFilterHandlers(byId("correctness-table"));
    }

    function renderCorrectnessOverview(payload, visibleTests, filters) {
        const target = byId("correctness-overview");
        const note = byId("correctness-table-note");
        if (!target) return;

        const summary = payload.summary || {};
        const tests = payload.tests || [];
        const layers = payload.layers || [];
        const statusCounts = countBy(tests, (item) => item.last_status || "unknown");
        const categoryCounts = countBy(tests, inlineTestCategory);
        const categoryEntries = sortedCountEntries(categoryCounts);
        const moduleCount = new Set(tests.map((item) => item.module).filter(Boolean)).size;
        const summaryTotal = summary.test_count ?? tests.length;
        const catalogMismatch = summaryTotal !== tests.length;
        const lastRun = summary.last_run || null;
        const lastRunStatus = lastRun?.status || "not run";
        const lastRunDuration = Number.isFinite(lastRun?.duration)
            ? `${formatNumber.format(lastRun.duration)}s`
            : "-";
        const runFailed = correctnessRunFailed(summary);
        const lastRunMessage = compactCorrectnessRunMessage(lastRun);
        const filterLabel = [
            filters.layerFilter ? `layer: ${filters.layerFilter}` : "all layers",
            filters.categoryFilter ? `category: ${filters.categoryFilter}` : "all categories",
            filters.query ? `search: ${filters.query}` : "no search",
            filters.showAll ? "all statuses" : "failed/unknown only",
        ].join(" · ");

        target.innerHTML = [
            correctnessOverviewCard("Catalog", catalogMismatch ? "warn" : "ok", [
                ["Summary tests", summaryTotal],
                ["Payload rows", tests.length],
                ["Modules", moduleCount],
                ["Layers", layers.length],
            ]),
            correctnessOverviewCard("Status", statusCounts.failed ? "bad" : statusCounts.unknown ? "warn" : "ok", [
                ["Passed", statusCounts.passed || 0],
                ["Failed", statusCounts.failed || 0],
                ["Skipped", statusCounts.skipped || 0],
                ["Unknown", statusCounts.unknown || 0],
            ]),
            correctnessCategoryCard("Inline Coverage", categoryEntries.length ? "ok" : "stale", categoryEntries, filters.categoryFilter),
            correctnessOverviewCard("Last Run", runFailed ? "bad" : lastRun?.status === "passed" ? "ok" : "stale", [
                ["Last run", lastRunStatus],
                ["Duration", lastRunDuration],
                ["Issue", runFailed ? lastRunMessage : "none"],
            ]),
            correctnessOverviewCard("Current View", visibleTests.length ? "ok" : "stale", [
                ["Visible rows", visibleTests.length],
                ["Filter", filters.showAll ? "all statuses" : "attention only"],
            ]),
        ].join("");

        if (note) {
            note.innerHTML = `<span>${escapeHtml(filterLabel)}</span>${catalogMismatch
                ? `<strong class="risk-warn">Catalog summary and payload row count differ.</strong>`
                : `<strong>${formatNumber.format(visibleTests.length)} of ${formatNumber.format(tests.length)} tests visible.</strong>`}`;
        }
        attachCorrectnessCategoryFilterHandlers(target);
    }

    function renderCorrectnessSummary(payload) {
        const target = byId("correctness-summary");
        if (!target) return;
        const summary = payload.summary || {};
        const layers = payload.layers || [];
        const tests = payload.tests || [];
        const statusCounts = countBy(tests, (item) => item.last_status || "unknown");
        const total = summary.test_count ?? tests.length;
        const failed = summary.failed ?? statusCounts.failed ?? 0;
        const unknown = summary.unknown ?? statusCounts.unknown ?? 0;
        const passed = statusCounts.passed ?? Math.max(0, total - failed - unknown);
        const passRate = total ? Math.round((passed / total) * 100) : 0;
        const categoryCounts = countBy(tests, inlineTestCategory);
        const categoryEntries = sortedCountEntries(categoryCounts);
        const layerCount = summary.layers ?? layers.length;
        const runFailed = correctnessRunFailed(summary);
        const lastRunMessage = compactCorrectnessRunMessage(correctnessLastRun(summary));
        const outcome = runFailed ? "run failed" : failed > 0 ? "failing" : unknown > 0 ? "needs review" : "passing";
        const heroTone = runFailed || failed > 0 ? "bad" : unknown > 0 ? "warn" : "ok";

        target.innerHTML = `<section class="correctness-summary-card correctness-summary-card--hero correctness-summary-card--${heroTone}">
                <span class="correctness-summary-card__label">Health</span>
                <strong>${escapeHtml(outcome)}</strong>
                <div class="correctness-pass-meter" aria-label="${passRate}% pass rate">
                    <span style="width:${passRate}%"></span>
                </div>
                <div class="correctness-summary-card__meta">
                    <span>${formatNumber.format(passed)} passed</span>
                    <span>${formatNumber.format(passRate)}% pass rate</span>
                </div>
            </section>
            ${correctnessSummaryCard("Tests", total, "Total cataloged test entries")}
            ${correctnessSummaryCard("Coverage Areas", categoryEntries.length, categoryEntries.slice(0, 2).map(([label, count]) => `${label} ${formatNumber.format(count)}`).join(" · ") || "No categories")}
            ${correctnessSummaryCard("Layers", layerCount, "Architectural groups represented")}
            ${correctnessSummaryCard(
                "Attention",
                runFailed ? "Run failed" : `${formatNumber.format(failed)} failed`,
                runFailed ? lastRunMessage : `${formatNumber.format(unknown)} unknown`,
                runFailed || failed ? "bad" : unknown ? "warn" : "ok"
            )}`;
    }

    function correctnessSummaryCard(label, value, detail, tone = "neutral") {
        return `<section class="correctness-summary-card correctness-summary-card--${escapeHtml(tone)}">
            <span class="correctness-summary-card__label">${escapeHtml(label)}</span>
            <strong>${escapeHtml(value)}</strong>
            <span class="correctness-summary-card__detail">${escapeHtml(detail)}</span>
        </section>`;
    }

    function correctnessOverviewCard(title, status, metrics) {
        return `<section class="correctness-overview-card correctness-overview-card--${escapeHtml(status)}">
            <div class="correctness-overview-card__header">
                <h3>${escapeHtml(title)}</h3>
                <span>${escapeHtml(status)}</span>
            </div>
            <div class="correctness-overview-card__metrics">
                ${metrics.map(([label, value]) => `<div>
                    <span>${escapeHtml(label)}</span>
                    <strong>${escapeHtml(value)}</strong>
                </div>`).join("")}
            </div>
        </section>`;
    }

    function correctnessCategoryCard(title, status, categories, activeCategory) {
        return `<section class="correctness-overview-card correctness-overview-card--${escapeHtml(status)}">
            <div class="correctness-overview-card__header">
                <h3>${escapeHtml(title)}</h3>
                <span>${escapeHtml(activeCategory || status)}</span>
            </div>
            <div class="correctness-category-grid">
                ${categories.map(([label, value]) => `<button type="button" class="correctness-category-chip ${activeCategory === label ? "is-active" : ""}" data-correctness-category="${escapeHtml(label)}">
                    <span>${escapeHtml(label)}</span>
                    <strong>${escapeHtml(value)}</strong>
                </button>`).join("")}
            </div>
        </section>`;
    }

    function attachCorrectnessCategoryFilterHandlers(root) {
        root?.querySelectorAll("[data-correctness-category]").forEach((button) => {
            button.addEventListener("click", () => {
                const category = button.dataset.correctnessCategory;
                state.selectedCorrectnessCategory = state.selectedCorrectnessCategory === category ? null : category;
                renderCorrectness();
            });
        });
    }

    function countBy(items, keyFn) {
        return items.reduce((counts, item) => {
            const key = keyFn(item);
            counts[key] = (counts[key] || 0) + 1;
            return counts;
        }, {});
    }

    function sortedCountEntries(counts) {
        return Object.entries(counts).sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
    }

    function inlineTestCategory(item) {
        const haystack = [
            item.name,
            item.description,
            item.module,
            item.path,
            item.layer,
        ].join(" ").toLowerCase().replace(/[_-]+/g, " ");

        if (/\b(text history|now line|timeline|follow focus|per file|chronological|entry ids collide|buffer id|global sequence)\b/.test(haystack)) {
            return "Text history UI";
        }
        if (/\b(undo|redo|typing|backspace|mistype|divider|coalesc|history entry|cursor jump|replay|seals|entry|transient edit)\b/.test(haystack)) {
            return "Undo and replay";
        }
        if (/\b(encoding|utf|windows_1252|save|snapshot|line ending|crlf|mixed)\b/.test(haystack)) {
            return "Encoding and save";
        }
        if (/\b(widget|raw egui ids|duplicate widget ids)\b/.test(haystack)) {
            return "Widget identity";
        }
        if (/\b(diagnostic|log|warning|logger|panic|egui id)\b/.test(haystack)) {
            return "Diagnostics";
        }
        if (/\b(count lines|leverage|iterator|unsafe|cfg test|paths are detected|test functions)\b/.test(haystack)) {
            return "Analysis tooling";
        }
        return "Behavior";
    }

    function renderRunLog() {
        const runs = [...state.runs].reverse();
        const running = runs.filter((item) => item.status === "running" || item.status === "queued").length;
        const failed = runs.filter((item) => item.status === "failed").length;
        const activeRun = runs.find((item) => item.status === "running" || item.status === "queued");
        renderSummary("run-log-summary", [
            metricCard("Runs", runs.length),
            metricCard("Running", running),
            metricCard("Failed", failed),
            metricCard("Latest", runs[0]?.status || "-"),
            activeProgressPill(activeRun),
        ]);
        renderTable(
            "run-log-table",
            ["Run", "Selector", "Tasks", "Progress", "Status", "Duration", "Artifacts"],
            runs.map((item) => `<tr class="run-row" data-run-id="${escapeHtml(item.id)}">
                <td><code>${escapeHtml(item.id)}</code></td>
                <td>${escapeHtml(item.selector || "-")}</td>
                <td>${renderPills(item.task_ids || [])}</td>
                <td>${renderRunProgress(item, "table")}</td>
                <td><span class="pill">${escapeHtml(item.status || "-")}</span></td>
                <td>${item.duration_seconds == null ? "-" : `${formatNumber.format(item.duration_seconds)} s`}</td>
                <td>${renderPills(item.artifacts || [])}</td>
            </tr>`)
        );
        byId("run-log-table").querySelectorAll(".run-row").forEach((row) => {
            row.addEventListener("click", () => loadRunLog(row.dataset.runId));
        });
        renderSelectedRunProgress();
    }

    function runProgress(run) {
        const taskIds = run.task_ids || [];
        const total = Number(run.total_tasks ?? taskIds.length ?? 0);
        const failed = Array.isArray(run.failed_task_ids) ? run.failed_task_ids.length : 0;
        let done = Number(run.completed_tasks ?? 0) + failed;
        if (!Number.isFinite(done)) done = 0;
        if (run.status === "completed" && total > 0) done = total;
        done = Math.max(0, Math.min(done, total));
        const left = Math.max(0, total - done);
        const percent = total > 0 ? Math.round((done / total) * 100) : 0;
        return { total, done, left, percent };
    }

    function runProgressLabel(run) {
        const progress = runProgress(run);
        if (!progress.total) return "-";
        const current = run.current_task_id ? ` · ${run.current_task_id}` : "";
        const detail = run.current_task_detail ? ` · ${run.current_task_detail}` : "";
        return `${progress.done}/${progress.total}${current}${detail}`;
    }

    function renderRunProgress(run, density = "detail") {
        const progress = runProgress(run);
        if (!progress.total) return '<span class="muted">-</span>';
        const statusClass = run.status === "failed"
            ? "is-failed"
            : run.status === "completed"
                ? "is-complete"
                : run.status === "interrupted"
                    ? "is-interrupted"
                    : "is-running";
        const current = run.current_task_id
            ? `<span class="run-progress__current">${escapeHtml(run.current_task_id)}</span>`
            : "";
        const detail = run.current_task_detail
            ? `<span class="run-progress__detail">${escapeHtml(run.current_task_detail)}</span>`
            : "";
        return `<div class="run-progress run-progress--${density} ${statusClass}">
            <div class="run-progress__track" role="progressbar" aria-valuenow="${progress.percent}" aria-valuemin="0" aria-valuemax="100" aria-label="${escapeHtml(run.id)} progress">
                <div class="run-progress__bar" style="width:${progress.percent}%"></div>
            </div>
            <div class="run-progress__meta">
                <span>${progress.done} done</span>
                <span>${progress.left} left</span>
                <span>${progress.percent}%</span>
            </div>
            ${current}
            ${detail}
        </div>`;
    }

    function renderSelectedRunProgress() {
        const target = byId("run-log-progress");
        if (!target) return;
        const run = state.runs.find((item) => item.id === state.selectedRun);
        target.innerHTML = run ? renderRunProgress(run, "detail") : "";
    }

    function localityRisk(item) {
        return Number(item?.non_locality_risk ?? item?.locality_risk ?? (100 - Number(item?.locality_score || 0)));
    }

    function leverageRisk(item) {
        return Number(item?.leverage_risk ?? (100 - Number(item?.leverage_score ?? item?.total_leverage_score ?? 0)));
    }

    function moduleRecord(rows, moduleName) {
        return rows.find((item) => (item.module_key || item.module_name) === moduleName) || null;
    }

    function localityForModule(node) {
        return node?.locality_metrics && Object.keys(node.locality_metrics).length
            ? node.locality_metrics
            : moduleRecord(state.locality, node?.id);
    }

    function leverageForModule(node) {
        return node?.leverage_metrics && Object.keys(node.leverage_metrics).length
            ? node.leverage_metrics
            : moduleRecord(state.leverage, node?.id);
    }

    function renderLocalityLeverageQuadrants(localityRows, leverageRows) {
        const target = byId("locality-leverage-quadrants");
        if (!target) return;
        const byModule = new Map();
        localityRows.forEach((item) => {
            byModule.set(item.module_key || item.module_name, { locality: item });
        });
        leverageRows.forEach((item) => {
            const key = item.module_key || item.module_name;
            const existing = byModule.get(key) || {};
            existing.leverage = item;
            byModule.set(key, existing);
        });
        const quadrants = {
            "low-risk-low-risk": { tone: "good", rows: [] },
            "low-locality-risk-high-leverage-risk": { tone: "local", rows: [] },
            "high-locality-risk-low-leverage-risk": { tone: "architecture", rows: [] },
            "high-risk-high-risk": { tone: "triage", rows: [] },
        };
        byModule.forEach((pair, moduleName) => {
            if (!pair.locality || !pair.leverage) return;
            const highLocalityRisk = localityRisk(pair.locality) >= 30;
            const highLeverageRisk = leverageRisk(pair.leverage) >= 40;
            const key = highLocalityRisk
                ? highLeverageRisk ? "high-risk-high-risk" : "high-locality-risk-low-leverage-risk"
                : highLeverageRisk ? "low-locality-risk-high-leverage-risk" : "low-risk-low-risk";
            quadrants[key].rows.push({
                moduleName,
                localityRisk: localityRisk(pair.locality),
                leverageRisk: leverageRisk(pair.leverage),
                localityScore: Number(pair.locality.locality_score ?? 0),
                leverageScore: Number(pair.leverage.leverage_score ?? pair.leverage.total_leverage_score ?? 0),
            });
        });
        const points = Object.values(quadrants)
            .flatMap((quadrant) => quadrant.rows.map((row) => ({ ...row, tone: quadrant.tone })));
        const highRisk = [...points]
            .map((item, index) => ({ ...item, pointIndex: index }))
            .sort((left, right) => (right.localityRisk + right.leverageRisk) - (left.localityRisk + left.leverageRisk))
            .slice(0, 10);
        const highRiskSet = new Set(highRisk.map((item) => item.pointIndex));
        const width = 760;
        const height = 360;
        const margin = { left: 74, right: 28, top: 32, bottom: 58 };
        const plotWidth = width - margin.left - margin.right;
        const plotHeight = height - margin.top - margin.bottom;
        const x = (value) => margin.left + (Math.max(0, Math.min(100, value)) / 100) * plotWidth;
        const y = (value) => margin.top + ((100 - Math.max(0, Math.min(100, value))) / 100) * plotHeight;
        const localityCut = x(30);
        const leverageCut = y(40);
        const pointNodes = points.map((item, index) => {
            const combined = item.localityRisk + item.leverageRisk;
            const radius = Math.max(4, Math.min(10, combined / 16));
            const className = `is-${item.tone}`;
            const topClass = highRiskSet.has(index) ? "is-top-risk" : "";
            return `<g class="ll-point ${className} ${topClass}" tabindex="0" role="button" data-point-index="${index}" aria-label="${escapeHtml(item.moduleName)}">
                <circle cx="${x(item.localityRisk)}" cy="${y(item.leverageRisk)}" r="${radius}"></circle>
            </g>`;
        }).join("");

        target.innerHTML = `<section class="panel-card ll-plot-card">
            <div class="panel-card__header">
                <div>
                    <h2>Locality / Leverage Map</h2>
                    <p>Each point is a module. Right means less local; up means weaker leverage tradeoff.</p>
                </div>
            </div>
            <div class="ll-plot-layout">
                <svg class="ll-plot" viewBox="0 0 ${width} ${height}" role="img" aria-label="Locality and leverage quadrant scatter plot">
                    <rect class="ll-quadrant ll-quadrant--local" x="${margin.left}" y="${margin.top}" width="${localityCut - margin.left}" height="${leverageCut - margin.top}"></rect>
                    <rect class="ll-quadrant ll-quadrant--triage" x="${localityCut}" y="${margin.top}" width="${margin.left + plotWidth - localityCut}" height="${leverageCut - margin.top}"></rect>
                    <rect class="ll-quadrant ll-quadrant--good" x="${margin.left}" y="${leverageCut}" width="${localityCut - margin.left}" height="${margin.top + plotHeight - leverageCut}"></rect>
                    <rect class="ll-quadrant ll-quadrant--architecture" x="${localityCut}" y="${leverageCut}" width="${margin.left + plotWidth - localityCut}" height="${margin.top + plotHeight - leverageCut}"></rect>
                    <line class="ll-threshold" x1="${localityCut}" x2="${localityCut}" y1="${margin.top}" y2="${margin.top + plotHeight}"></line>
                    <line class="ll-threshold" x1="${margin.left}" x2="${margin.left + plotWidth}" y1="${leverageCut}" y2="${leverageCut}"></line>
                    <line class="ll-axis" x1="${margin.left}" x2="${margin.left}" y1="${margin.top}" y2="${margin.top + plotHeight}"></line>
                    <line class="ll-axis" x1="${margin.left}" x2="${margin.left + plotWidth}" y1="${margin.top + plotHeight}" y2="${margin.top + plotHeight}"></line>
                    <text class="ll-axis-label" x="${margin.left + plotWidth / 2}" y="${height - 16}">Non-locality risk</text>
                    <text class="ll-axis-label ll-axis-label--y" x="20" y="${margin.top + plotHeight / 2}">Leverage risk</text>
                    <text class="ll-tick" x="${x(0)}" y="${height - 38}">0</text>
                    <text class="ll-tick" x="${x(30)}" y="${height - 38}">30</text>
                    <text class="ll-tick" x="${x(100)}" y="${height - 38}">100</text>
                    <text class="ll-tick" x="${margin.left - 28}" y="${y(100) + 4}">100</text>
                    <text class="ll-tick" x="${margin.left - 22}" y="${y(40) + 4}">40</text>
                    <text class="ll-tick" x="${margin.left - 14}" y="${y(0) + 4}">0</text>
                    ${pointNodes}
                </svg>
                <div class="ll-popover" hidden></div>
                <div class="ll-ranked-list">
                    <h3>Worst combined risk</h3>
                    ${highRisk.map((item, index) => `<button type="button" class="ll-ranked-row" data-point-index="${item.pointIndex}" title="${escapeHtml(item.moduleName)}">
                        <span>${index + 1}</span>
                        <code>${escapeHtml(shortenLabel(item.moduleName))}</code>
                        <strong>${formatNumber.format(item.localityRisk + item.leverageRisk)}</strong>
                    </button>`).join("")}
                </div>
            </div>
        </section>`;

        const popover = target.querySelector(".ll-popover");
        const setActivePoint = (index) => {
            target.querySelectorAll(".ll-point").forEach((point) => {
                point.classList.toggle("is-active", Number(point.dataset.pointIndex) === index);
            });
            target.querySelectorAll(".ll-ranked-row").forEach((row) => {
                row.classList.toggle("is-active", Number(row.dataset.pointIndex) === index);
            });
        };
        const showPopover = (index) => {
            const item = points[index];
            if (!item || !popover) return;
            setActivePoint(index);
            const left = (x(item.localityRisk) / width) * 100;
            const top = (y(item.leverageRisk) / height) * 100;
            popover.hidden = false;
            popover.classList.toggle("ll-popover--left", left > 68);
            popover.classList.toggle("ll-popover--top", top < 24);
            popover.classList.toggle("ll-popover--bottom", top > 76);
            popover.style.left = `${left}%`;
            popover.style.top = `${top}%`;
            popover.innerHTML = `<strong>${escapeHtml(item.moduleName)}</strong>
                <div><span>Locality risk</span><b>${formatNumber.format(item.localityRisk)}</b></div>
                <div><span>Leverage risk</span><b>${formatNumber.format(item.leverageRisk)}</b></div>
                <div><span>Locality score</span><b>${formatNumber.format(item.localityScore)}</b></div>
                <div><span>Leverage score</span><b>${formatNumber.format(item.leverageScore)}</b></div>`;
        };
        target.querySelectorAll(".ll-point").forEach((point) => {
            point.addEventListener("click", (event) => {
                event.stopPropagation();
                showPopover(Number(point.dataset.pointIndex));
            });
            point.addEventListener("keydown", (event) => {
                if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    showPopover(Number(point.dataset.pointIndex));
                }
            });
        });
        target.querySelectorAll(".ll-ranked-row").forEach((row) => {
            row.addEventListener("click", (event) => {
                event.stopPropagation();
                showPopover(Number(row.dataset.pointIndex));
            });
        });
        target.querySelector(".ll-plot-layout")?.addEventListener("click", () => {
            if (popover) popover.hidden = true;
            setActivePoint(-1);
        });
    }

    function renderLocalityLeverage() {
        const localityTarget = byId("locality-table");
        const leverageTarget = byId("leverage-table");
        if (!localityTarget || !leverageTarget) return;

        const localityQuery = byId("locality-filter")?.value || "";
        const leverageQuery = byId("leverage-filter")?.value || "";

        const filteredLocality = state.locality.filter((item) => matchesFilter(item, localityQuery));
        const filteredLeverage = state.leverage.filter((item) => matchesFilter(item, leverageQuery));
        renderLocalityLeverageQuadrants(state.locality, state.leverage);
        renderChurnComplexityScatter();
        renderModuleQualityPivot();
        const unavailableStyleRows = state.leverage.filter((item) => (item.parse_status || "ok") !== "ok");
        const leverageStatus = byId("leverage-status");
        if (leverageStatus) {
            leverageStatus.innerHTML = unavailableStyleRows.length
                ? `<div class="quality-status-banner quality-status-banner--warn"><strong>Style data unavailable for ${formatNumber.format(unavailableStyleRows.length)} modules</strong><span>${escapeHtml([...new Set(unavailableStyleRows.map((item) => item.parse_status || "not_measured"))].join(", "))}</span></div>`
                : "";
        }

        const localityDistItems = state.locality.map((item) => ({
            key: `locality:${item.module_key || item.module_name}`,
            kind: "locality",
            name: item.module_key || item.module_name,
            score: localityRisk(item),
            signals: item.signals || [],
            details: `Far deps ${formatNumber.format(item.far_dependencies || 0)} · hidden ${formatNumber.format(item.hidden_coupling_count || 0)}`,
            raw: item,
            searchText: [item.module_key, item.module_name, item.path, item.test_locality, ...item.signals].join(" "),
        }));

        const leverageDistItems = state.leverage.map((item) => ({
            key: `leverage:${item.module_key || item.module_name}`,
            kind: "leverage",
            name: item.module_key || item.module_name,
            score: leverageRisk(item),
            signals: item.signals || [],
            details: `Reach ${formatNumber.format(item.reach || 0)} · ripple ${formatNumber.format(item.avg_cochanged_modules || 0)}`,
            raw: item,
            searchText: [item.module_key, item.module_name, item.path, ...item.signals].join(" "),
        }));

        renderRiskDistribution(byId("locality-distribution"), localityDistItems, {
            empty: "No locality data.",
            modeKey: "localityDistributionMode",
            expandedKey: "expandedLocalityKey",
            warn: 15,
            bad: 30,
            scoreLabel: "locality risk",
            dataset: "locality",
            filterField: "risk",
        });

        renderRiskDistribution(byId("leverage-distribution"), leverageDistItems, {
            empty: "No leverage data.",
            modeKey: "leverageDistributionMode",
            expandedKey: "expandedLeverageKey",
            warn: 20,
            bad: 40,
            scoreLabel: "leverage risk",
            dataset: "leverage",
            filterField: "risk",
        });

        renderTable(
            "locality-table",
            ["Rank", "Module", "Path", "Locality", "Risk", "Far Deps", "Hidden", "Explicit", "Out/In", "Churn", "Tests", "Signals"],
            filteredLocality.map((item, index) => {
                const risk = localityRisk(item);
                const scoreClass = riskClass(risk, 15, 30);
                const moduleName = item.module_key || item.module_name;
                return `<tr>
                    <td>${index + 1}</td>
                    <td><code>${escapeHtml(moduleName)}</code></td>
                    <td><code>${escapeHtml(item.path || "")}</code></td>
                    <td class="${scoreClass}">${formatNumber.format(item.locality_score)}</td>
                    <td class="${scoreClass}">${formatNumber.format(risk)}</td>
                    <td>${formatNumber.format(item.far_dependencies || 0)}</td>
                    <td>${formatNumber.format(item.hidden_coupling_count || 0)}</td>
                    <td>${formatNumber.format(item.interface_explicitness_ratio ?? 0)}</td>
                    <td>${formatNumber.format(item.outbound_dependencies || 0)} / ${formatNumber.format(item.inbound_dependencies || 0)}</td>
                    <td>${formatNumber.format(item.churn || 0)}</td>
                    <td>${escapeHtml(item.test_locality || "-")}</td>
                    <td>${renderPills(item.signals)}</td>
                </tr>`;
            })
        );

        renderTable(
            "leverage-table",
            ["Rank", "Module", "Path", "Leverage", "Risk", "Reach", "Areas", "Invariant", "Divergence", "Ripple", "Style", "Signals"],
            filteredLeverage.map((item, index) => {
                const risk = leverageRisk(item);
                const score = item.leverage_score ?? item.total_leverage_score ?? 0;
                const scoreClass = riskClass(risk, 20, 40);
                const moduleName = item.module_key || item.module_name;
                const path = item.path || item.module_name;
                return `<tr>
                    <td>${index + 1}</td>
                    <td><code>${escapeHtml(moduleName)}</code></td>
                    <td><code>${escapeHtml(path)}</code></td>
                    <td class="${scoreClass}">${formatNumber.format(score)}</td>
                    <td class="${scoreClass}">${formatNumber.format(risk)}</td>
                    <td>${formatNumber.format(item.reach || 0)}</td>
                    <td>${formatNumber.format(item.caller_area_count || 0)}</td>
                    <td>${formatNumber.format(item.invariant_surface || 0)}</td>
                    <td>${formatNumber.format(item.divergence_count || 0)}</td>
                    <td>${formatNumber.format(item.avg_cochanged_modules || 0)}</td>
                    <td>${formatNumber.format(item.style_leverage_score ?? item.iterator_leverage_score ?? 0)}</td>
                    <td>${renderPills(item.signals)}</td>
                </tr>`;
            })
        );
    }

    function moduleKeyFromPath(value) {
        const normalized = normalizePath(value).replace(/^.*?src\//, "").replace(/\.rs$/, "");
        if (!normalized) return "";
        return normalized.endsWith("/mod") ? normalized.slice(0, -4).replaceAll("/", "::") : normalized.replaceAll("/", "::");
    }

    function moduleKeyForRecord(item) {
        return item.module_key || item.module_name || moduleKeyFromPath(item.path || item.name || item.file_path || "");
    }

    function renderChurnComplexityScatter() {
        const target = byId("churn-complexity-scatter");
        if (!target) return;
        const localityByModule = new Map((state.locality || []).map((item) => [moduleKeyForRecord(item), item]));
        const points = (state.hotspots || [])
            .filter((item) => item.kind === "unit")
            .map((item) => {
                const moduleKey = moduleKeyFromPath(item.name);
                const locality = localityByModule.get(moduleKey);
                return {
                    name: item.name,
                    moduleKey,
                    x: Number(locality?.churn || 0),
                    y: qualityScore(item),
                    sloc: Number(item.sloc || 0),
                };
            })
            .filter((item) => item.x > 0 || item.y > 0);
        if (!points.length) {
            target.innerHTML = `<p class="muted">No joined churn and hotspot data.</p>`;
            return;
        }
        const width = 940;
        const height = 430;
        const margin = { left: 58, right: 28, top: 30, bottom: 64 };
        const plotWidth = width - margin.left - margin.right;
        const plotHeight = height - margin.top - margin.bottom;
        const maxChurn = Math.max(1, ...points.map((item) => item.x));
        const minQuality = Math.max(0, Math.min(...points.map((item) => item.y), 300) - 40);
        const maxQuality = Math.max(1, 600, ...points.map((item) => item.y)) * 1.05;
        const x = (value) => margin.left + (Math.log1p(Math.max(0, value)) / Math.log1p(maxChurn)) * plotWidth;
        const y = (value) => margin.top + (1 - ((clamp(value, minQuality, maxQuality) - minQuality) / Math.max(maxQuality - minQuality, 1))) * plotHeight;
        const plotPoints = points.map((item, index) => ({ ...item, pointIndex: index }));
        const highRisk = [...plotPoints].sort((a, b) => (b.x * b.y) - (a.x * a.y)).slice(0, 8);
        const highRiskKeys = new Set(highRisk.map((item) => item.name));
        const churnTicks = [0, Math.round(maxChurn / 4), Math.round(maxChurn / 2), maxChurn]
            .filter((value, index, values) => values.indexOf(value) === index);
        const qualityTicks = [Math.ceil(minQuality / 100) * 100, 300, 450, 600, Math.floor(maxQuality / 100) * 100]
            .filter((value, index, values) => value >= minQuality && value <= maxQuality && values.indexOf(value) === index);
        const gridLines = [
            ...churnTicks.map((value) => `<line class="ll-grid" x1="${x(value).toFixed(1)}" x2="${x(value).toFixed(1)}" y1="${margin.top}" y2="${margin.top + plotHeight}"></line>
                <text class="ll-tick" x="${x(value).toFixed(1)}" y="${height - 38}" text-anchor="middle">${formatNumber.format(value)}</text>`),
            ...qualityTicks.map((value) => `<line class="ll-grid" x1="${margin.left}" x2="${margin.left + plotWidth}" y1="${y(value).toFixed(1)}" y2="${y(value).toFixed(1)}"></line>
                <text class="ll-tick ll-tick--y" x="${margin.left - 16}" y="${y(value).toFixed(1)}">${formatNumber.format(value)}</text>`),
        ].join("");
        const dots = plotPoints.map((item) => {
            const cls = item.y >= 600 ? "bad" : item.y >= 300 ? "warn" : "good";
            const radius = Math.max(4, Math.min(12, 3 + Math.sqrt(item.sloc || 1) / 4));
            const topClass = highRiskKeys.has(item.name) ? "is-top-risk" : "";
            const label = `${item.moduleKey || item.name}: churn ${formatNumber.format(item.x)}, quality ${formatNumber.format(item.y)}`;
            return `<circle class="type-health-point churn-complexity-point type-health-point--${cls} ${topClass}" cx="${x(item.x).toFixed(1)}" cy="${y(item.y).toFixed(1)}" r="${radius.toFixed(1)}" tabindex="0" role="button" data-churn-point-index="${item.pointIndex}" aria-label="${escapeHtml(label)}">
                <title>${escapeHtml(label)}</title>
            </circle>`;
        }).join("");
        target.innerHTML = `<div class="churn-scatter-layout">
            <svg class="type-health-scatter__plot" viewBox="0 0 ${width} ${height}" role="img" aria-label="Churn versus complexity hotspot scatter plot">
                ${gridLines}
                <line class="ll-axis" x1="${margin.left}" x2="${margin.left}" y1="${margin.top}" y2="${margin.top + plotHeight}"></line>
                <line class="ll-axis" x1="${margin.left}" x2="${margin.left + plotWidth}" y1="${margin.top + plotHeight}" y2="${margin.top + plotHeight}"></line>
                <line class="ll-threshold" x1="${margin.left}" x2="${margin.left + plotWidth}" y1="${y(300)}" y2="${y(300)}"></line>
                <text class="ll-axis-label" x="${margin.left + plotWidth / 2}" y="${height - 16}">Churn (log)</text>
                <text class="ll-axis-label ll-axis-label--y" x="20" y="${margin.top + plotHeight / 2}">Quality score</text>
                ${dots}
            </svg>
            <div class="ll-popover churn-popover" hidden></div>
            <div class="ll-ranked-list churn-ranked-list">
                <h3>Refactor targets</h3>
                ${highRisk.map((item, index) => `<button type="button" class="ll-ranked-row" data-churn-point-index="${item.pointIndex}" title="${escapeHtml(item.name)}">
                    <span>${index + 1}</span>
                    <code>${escapeHtml(shortenLabel(item.moduleKey || item.name))}</code>
                    <strong>${formatNumber.format(item.y)}</strong>
                </button>`).join("")}
            </div>
        </div>`;
        const popover = target.querySelector(".churn-popover");
        const setActivePoint = (index) => {
            target.querySelectorAll("[data-churn-point-index]").forEach((element) => {
                element.classList.toggle("is-active", Number(element.dataset.churnPointIndex) === index);
            });
        };
        const hidePopover = () => {
            if (popover) popover.hidden = true;
            setActivePoint(-1);
        };
        const showPopover = (index) => {
            const item = plotPoints[index];
            if (!item || !popover) return;
            const dotX = x(item.x);
            const dotY = y(item.y);
            const left = (dotX / width) * 100;
            const top = (dotY / height) * 100;
            const cls = riskClass(item.y, 300, 600);
            setActivePoint(index);
            popover.hidden = false;
            popover.classList.toggle("ll-popover--left", left > 68);
            popover.classList.toggle("ll-popover--top", top < 24);
            popover.classList.toggle("ll-popover--bottom", top > 76);
            popover.style.left = `${left}%`;
            popover.style.top = `${top}%`;
            popover.innerHTML = `<strong>${escapeHtml(item.moduleKey || item.name)}</strong>
                <code>${escapeHtml(item.name)}</code>
                <div><span>Quality score</span><b class="${cls}">${formatNumber.format(item.y)}</b></div>
                <div><span>Churn</span><b>${formatNumber.format(item.x)}</b></div>
                <div><span>SLOC</span><b>${formatNumber.format(item.sloc)}</b></div>
                <button type="button" class="performance-bin-popover__action" data-quality-bin-dataset="hotspots" data-quality-bin-filter="${escapeHtml(item.name)}">Show in Hotspots</button>`;
        };
        target.querySelectorAll("[data-churn-point-index]").forEach((element) => {
            element.addEventListener("click", (event) => {
                event.stopPropagation();
                showPopover(Number(element.dataset.churnPointIndex));
            });
            element.addEventListener("keydown", (event) => {
                if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    showPopover(Number(element.dataset.churnPointIndex));
                }
                if (event.key === "Escape") {
                    hidePopover();
                }
            });
        });
        popover?.addEventListener("click", (event) => {
            const action = event.target.closest("[data-quality-bin-dataset]");
            if (action) {
                event.stopPropagation();
                drillToQualityDataset(action.dataset.qualityBinDataset, action.dataset.qualityBinFilter || "");
            }
        });
        target.querySelector(".churn-scatter-layout")?.addEventListener("click", () => {
            hidePopover();
        });
    }

    function renderModuleQualityPivot() {
        const target = byId("quality-module-pivot");
        if (!target) return;
        const modules = new Map();
        const rowFor = (moduleKey) => {
            const key = moduleKey || "unknown";
            const row = modules.get(key) || { moduleKey: key, hotspots: 0, worstQuality: 0, clones: 0, typeRisk: 0, escapeScore: 0, localityRisk: 0, leverageRisk: 0 };
            modules.set(key, row);
            return row;
        };
        (state.hotspots || []).filter((item) => item.kind === "unit").forEach((item) => {
            const row = rowFor(moduleKeyFromPath(item.name));
            row.hotspots += 1;
            row.worstQuality = Math.max(row.worstQuality, qualityScore(item));
        });
        (state.clones || []).forEach((clone) => {
            const seen = new Set((clone.instances || []).map((inst) => moduleKeyFromPath(inst.file_path)).filter(Boolean));
            seen.forEach((moduleKey) => { rowFor(moduleKey).clones += 1; });
        });
        (state.typeHealth || []).forEach((item) => {
            const row = rowFor(moduleKeyForRecord(item));
            row.typeRisk = Math.max(row.typeRisk, typeHealthRisk(item));
        });
        (state.escapeHatches || []).forEach((item) => {
            const row = rowFor(moduleKeyForRecord(item));
            row.escapeScore = Math.max(row.escapeScore, Number(item.escape_hatch_score || 0));
        });
        (state.locality || []).forEach((item) => { rowFor(moduleKeyForRecord(item)).localityRisk = localityRisk(item); });
        (state.leverage || []).forEach((item) => { rowFor(moduleKeyForRecord(item)).leverageRisk = leverageRisk(item); });
        const sort = state.moduleRollupSort || { key: "combined", direction: "desc" };
        const sortSign = sort.direction === "asc" ? 1 : -1;
        const rows = [...modules.values()]
            .map((row) => ({ ...row, combined: normalizedRisk(row.worstQuality, 300, 600) + normalizedRisk(row.clones, 2, 6) + normalizedRisk(row.typeRisk, 40, 70) + normalizedRisk(row.escapeScore, 20, 50) + normalizedRisk(row.localityRisk, 15, 30) + normalizedRisk(row.leverageRisk, 20, 40) }))
            .sort((a, b) => {
                if (sort.key === "moduleKey") {
                    return a.moduleKey.localeCompare(b.moduleKey) * sortSign;
                }
                return (Number(a[sort.key] || 0) - Number(b[sort.key] || 0)) * sortSign;
            })
            .slice(0, 32);
        const columns = [
            ["moduleKey", "Module"],
            ["combined", "Combined"],
            ["hotspots", "Hotspots"],
            ["worstQuality", "Worst Quality"],
            ["clones", "Clone Touches"],
            ["typeRisk", "Type Risk"],
            ["escapeScore", "Escape"],
            ["localityRisk", "Locality"],
            ["leverageRisk", "Leverage"],
        ];
        const header = columns.map(([key, label]) => {
            const active = sort.key === key;
            const nextDirection = active && sort.direction === "desc" ? "asc" : "desc";
            const ariaSort = active ? (sort.direction === "desc" ? "descending" : "ascending") : "none";
            const indicator = active ? (sort.direction === "desc" ? "v" : "^") : "";
            return `<th aria-sort="${ariaSort}">
                <button type="button" class="table-sort-button ${active ? "is-active" : ""}" data-module-rollup-sort="${escapeHtml(key)}" data-module-rollup-direction="${escapeHtml(nextDirection)}">
                    <span>${escapeHtml(label)}</span><i>${escapeHtml(indicator)}</i>
                </button>
            </th>`;
        }).join("");
        const body = rows.length
            ? rows.map((row) => `<tr>
                <td><code>${escapeHtml(row.moduleKey)}</code></td>
                <td class="${riskClass(row.combined, 180, 300)}">${formatNumber.format(row.combined)}</td>
                <td>${formatNumber.format(row.hotspots)}</td>
                <td class="${riskClass(row.worstQuality, 300, 600)}">${formatNumber.format(row.worstQuality)}</td>
                <td>${formatNumber.format(row.clones)}</td>
                <td class="${riskClass(row.typeRisk, 40, 70)}">${formatNumber.format(row.typeRisk)}</td>
                <td class="${riskClass(row.escapeScore, 20, 50)}">${formatNumber.format(row.escapeScore)}</td>
                <td class="${riskClass(row.localityRisk, 15, 30)}">${formatNumber.format(row.localityRisk)}</td>
                <td class="${riskClass(row.leverageRisk, 20, 40)}">${formatNumber.format(row.leverageRisk)}</td>
            </tr>`).join("")
            : `<tr><td colspan="${columns.length}" class="muted">No data loaded.</td></tr>`;
        target.innerHTML = `<table><thead><tr>${header}</tr></thead><tbody>${body}</tbody></table>`;
        target.querySelectorAll("[data-module-rollup-sort]").forEach((button) => {
            button.addEventListener("click", () => {
                state.moduleRollupSort = {
                    key: button.dataset.moduleRollupSort || "combined",
                    direction: button.dataset.moduleRollupDirection || "desc",
                };
                renderModuleQualityPivot();
            });
        });
    }

    function renderMap() {
        const payload = state.map;
        if (!payload?.graph) {
            renderSummary("map-summary", [
                metricCard("Nodes", "-"),
                metricCard("Edges", "-"),
                metricCard("High maintainability", "-"),
                metricCard("Untested risk", "-"),
            ]);
            byId("map-graph").innerHTML = '<p class="muted" style="padding: 20px;">No map data loaded.</p>';
            return;
        }

        const query = byId("map-filter").value.toLowerCase();
        const graph = payload.graph;
        let modules = graph.nodes
            .map((node) => node.data)
            .filter((node) => !node.is_group)
            .filter((node) => !query || node.id.toLowerCase().includes(query));

        if (state.focusMode && state.selectedModule) {
            const focusIds = new Set([state.selectedModule]);
            graph.edges.forEach((edge) => {
                if (edge.data.source === state.selectedModule) focusIds.add(edge.data.target);
                if (edge.data.target === state.selectedModule) focusIds.add(edge.data.source);
            });
            modules = modules.filter((node) => focusIds.has(node.id));
        }

        const moduleIds = new Set(modules.map((node) => node.id));
        const summary = payload.meta?.summary || {};
        const highMaintainability = modules.filter((node) => (node.maintainability_risk || 0) >= 350).length;
        const lowTestEvidence = modules.filter((node) => !node.evidence?.has_tests).length;
        const visibleEdges = graph.edges
            .map((edge) => edge.data)
            .filter((edge) => moduleIds.has(edge.source) && moduleIds.has(edge.target));

        renderSummary("map-summary", [
            metricCard("Nodes", modules.length),
            metricCard("Edges", visibleEdges.length),
            metricCard("High maintainability", highMaintainability),
            metricCard("Untested risk", lowTestEvidence),
            metricCard("Cycle members", summary.cycle_members ?? "-"),
            metricCard("Selected", state.selectedModule || "-"),
        ]);

        const layout = buildMapLayout(modules);
        const rowMarkup = renderFolderRows(layout);
        const edgeMarkup = visibleEdges.map((edge) => renderEdge(edge, layout)).join("");
        const nodeMarkup = modules.map((node) => renderNode(node, layout)).join("");
        const width = Math.max(1200, layout.width);
        const height = Math.max(720, layout.height);
        const displayWidth = Math.round(width * state.mapZoom);
        const displayHeight = Math.round(height * state.mapZoom);

        byId("map-graph").classList.toggle("has-selection", Boolean(state.selectedModule));
        byId("map-graph").innerHTML = `<svg class="map-svg" width="${displayWidth}" height="${displayHeight}" viewBox="0 0 ${width} ${height}" role="img" aria-label="Architecture dependency map">
            <defs>
                <marker id="arrow-muted" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
                    <path d="M 0 0 L 10 5 L 0 10 z" fill="rgba(159, 176, 195, 0.35)"></path>
                </marker>
                <marker id="arrow-outbound" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
                    <path d="M 0 0 L 10 5 L 0 10 z" fill="#7ddc9b"></path>
                </marker>
                <marker id="arrow-inbound" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
                    <path d="M 0 0 L 10 5 L 0 10 z" fill="#ff7474"></path>
                </marker>
            </defs>
            <g>${rowMarkup}</g>
            <g>${edgeMarkup}</g>
            <g>${nodeMarkup}</g>
        </svg>`;

        byId("map-graph").querySelectorAll(".map-node").forEach((node) => {
            node.addEventListener("click", () => {
                const nodeId = node.getAttribute("data-id");
                state.selectedModule = state.selectedModule === nodeId ? null : nodeId;
                renderMap();
            });
        });

        renderMapDetail(modules, visibleEdges);
    }

    function buildMapLayout(nodes) {
        const groups = new Map();
        const groupNames = new Set();

        if (state.mapLayout === 'layer') {
            nodes.forEach((node) => {
                const layer = node.layer || 'default';
                groupNames.add(layer);
                if (!groups.has(layer)) groups.set(layer, []);
                groups.get(layer).push(node);
            });
            const layerOrder = ["chrome", "ui", "services", "domain", "app_state", "default"];
            const orderedNames = Array.from(groupNames).sort((a, b) => {
                const idxA = layerOrder.indexOf(a);
                const idxB = layerOrder.indexOf(b);
                if (idxA !== -1 && idxB !== -1) return idxA - idxB;
                if (idxA !== -1) return -1;
                if (idxB !== -1) return 1;
                return a.localeCompare(b);
            });
            groupNames.clear();
            orderedNames.forEach(n => groupNames.add(n));
        } else {
            groupNames.add("src");
            nodes.forEach((node) => {
                folderAncestors(node.id).forEach((folder) => groupNames.add(folder));
                const folder = folderPathForModule(node.id);
                if (!groups.has(folder)) {
                    groups.set(folder, []);
                }
                groups.get(folder).push(node);
            });
            const orderedFoldersArr = orderedFolders(groupNames);
            groupNames.clear();
            orderedFoldersArr.forEach(n => groupNames.add(n));
        }

        const nodeWidth = 260;
        const nodeHeight = 88;
        const positions = new Map();
        const rows = [];
        let mapWidth = 0;
        let mapHeight = 0;

        if (state.mapLayout === 'layer') {
            const colWidth = nodeWidth + 60;
            const yGap = 20;
            const topOffset = 76;
            const leftOffset = 40;

            let maxModulesInCol = 0;
            const orderedGroups = Array.from(groupNames);
            orderedGroups.forEach((group) => {
                maxModulesInCol = Math.max(maxModulesInCol, (groups.get(group) || []).length);
            });

            const colHeight = topOffset + maxModulesInCol * (nodeHeight + yGap) + 40;
            mapWidth = leftOffset + orderedGroups.length * colWidth + 40;
            mapHeight = colHeight + 60;

            orderedGroups.forEach((group, colIndex) => {
                const modules = groups.get(group) || [];
                const colX = leftOffset + colIndex * colWidth;

                rows.push({
                    isColumn: true,
                    folder: group,
                    x: colX,
                    y: 30,
                    width: colWidth - 20,
                    height: colHeight,
                    label: group,
                    modules: modules,
                });

                modules
                    .sort((left, right) => {
                        const metricRight = state.mapMetric === 'maintainability' ? right.maintainability_risk :
                            state.mapMetric === 'change' ? right.change_risk :
                                state.mapMetric === 'performance' ? right.performance_risk :
                                    state.mapMetric === 'quality' ? right.quality_risk :
                                        state.mapMetric === 'correctness' ? right.correctness_risk :
                                            state.mapMetric === 'architectural' ? right.architectural_risk :
                                                state.mapMetric === 'churn' ? right.churn : right.total_score;
                        const metricLeft = state.mapMetric === 'maintainability' ? left.maintainability_risk :
                            state.mapMetric === 'change' ? left.change_risk :
                                state.mapMetric === 'performance' ? left.performance_risk :
                                    state.mapMetric === 'quality' ? left.quality_risk :
                                        state.mapMetric === 'correctness' ? left.correctness_risk :
                                            state.mapMetric === 'architectural' ? left.architectural_risk :
                                                state.mapMetric === 'churn' ? left.churn : left.total_score;
                        return (metricRight || 0) - (metricLeft || 0);
                    })
                    .forEach((node, moduleIndex) => {
                        positions.set(node.id, {
                            x: colX + 10,
                            y: topOffset + moduleIndex * (nodeHeight + yGap),
                            folder: group,
                            width: nodeWidth,
                            height: nodeHeight,
                        });
                    });
            });
        } else {
            const rowHeight = 134;
            const xGap = 34;
            const topOffset = 76;
            const leftOffset = 300;
            let maxColumns = 1;

            Array.from(groupNames).forEach((group, rowIndex) => {
                const modules = groups.get(group) || [];
                maxColumns = Math.max(maxColumns, modules.length);
                const rowY = topOffset + rowIndex * rowHeight;

                rows.push({
                    isColumn: false,
                    folder: group,
                    y: rowY,
                    height: rowHeight - 18,
                    label: folderLabel(group),
                    modules: modules,
                });

                modules
                    .sort((left, right) => {
                        const metricRight = state.mapMetric === 'maintainability' ? right.maintainability_risk :
                            state.mapMetric === 'change' ? right.change_risk :
                                state.mapMetric === 'performance' ? right.performance_risk :
                                    state.mapMetric === 'quality' ? right.quality_risk :
                                        state.mapMetric === 'correctness' ? right.correctness_risk :
                                            state.mapMetric === 'architectural' ? right.architectural_risk :
                                                state.mapMetric === 'churn' ? right.churn : right.total_score;
                        const metricLeft = state.mapMetric === 'maintainability' ? left.maintainability_risk :
                            state.mapMetric === 'change' ? left.change_risk :
                                state.mapMetric === 'performance' ? left.performance_risk :
                                    state.mapMetric === 'quality' ? left.quality_risk :
                                        state.mapMetric === 'correctness' ? left.correctness_risk :
                                            state.mapMetric === 'architectural' ? left.architectural_risk :
                                                state.mapMetric === 'churn' ? left.churn : left.total_score;
                        return (metricRight || 0) - (metricLeft || 0);
                    })
                    .forEach((node, columnIndex) => {
                        positions.set(node.id, {
                            x: leftOffset + columnIndex * (nodeWidth + xGap),
                            y: rowY + 14,
                            folder: group,
                            width: nodeWidth,
                            height: nodeHeight,
                        });
                    });
            });

            mapWidth = leftOffset + Math.max(maxColumns, 2) * (nodeWidth + xGap) + 80;
            mapHeight = topOffset + rows.length * rowHeight + 70;
        }

        return {
            positions,
            rows,
            width: mapWidth,
            height: mapHeight,
        };
    }

    function folderAncestors(moduleId) {
        const parts = moduleId.split("::");
        const ancestors = ["src"];
        for (let index = 1; index < parts.length; index += 1) {
            ancestors.push(parts.slice(0, index).join("::"));
        }
        return ancestors;
    }

    function folderPathForModule(moduleId) {
        const parts = moduleId.split("::");
        if (parts.length <= 1) {
            return "src";
        }
        return parts.slice(0, -1).join("::");
    }

    function orderedFolders(folderNames) {
        return Array.from(folderNames).sort((left, right) => {
            if (left === "src") {
                return -1;
            }
            if (right === "src") {
                return 1;
            }
            return left.localeCompare(right);
        });
    }

    function folderDepth(folder) {
        if (folder === "src") {
            return 0;
        }
        return folder.split("::").length;
    }

    function folderLabel(folder) {
        if (folder === "src") {
            return "src";
        }
        return `${"  ".repeat(Math.max(0, folderDepth(folder) - 1))}${folder}`;
    }

    function renderFolderRows(layout) {
        return layout.rows
            .map((row, index) => {
                const tone = index % 2 === 0 ? "rgba(255,255,255,0.035)" : "rgba(255,255,255,0.015)";
                if (row.isColumn) {
                    return `<g class="folder-row" transform="translate(${row.x - 10} ${row.y})">
                        <rect width="${row.width}" height="${row.height}" rx="18" fill="${tone}"></rect>
                        <foreignObject x="18" y="20" width="${row.width - 36}" height="76">
                            <div xmlns="http://www.w3.org/1999/xhtml" class="folder-label">
                                <strong>${escapeHtml(row.label)}</strong>
                                <span>${row.modules.length} modules</span>
                            </div>
                        </foreignObject>
                    </g>`;
                } else {
                    const width = Math.max(900, layout.width - 60);
                    return `<g class="folder-row" transform="translate(30 ${row.y - 10})">
                        <rect width="${width}" height="${row.height}" rx="18" fill="${tone}"></rect>
                        <foreignObject x="18" y="20" width="218" height="76">
                            <div xmlns="http://www.w3.org/1999/xhtml" class="folder-label">
                                <strong>${escapeHtml(row.label)}</strong>
                                <span>${row.modules.length} modules</span>
                            </div>
                        </foreignObject>
                    </g>`;
                }
            }).join("");
    }

    function renderEdge(edge, layout) {
        const source = layout.positions.get(edge.source);
        const target = layout.positions.get(edge.target);
        if (!source || !target) {
            return "";
        }

        const selected = state.selectedModule;
        const className = [
            "map-link",
            selected === edge.source ? "is-outbound" : "",
            selected === edge.target ? "is-inbound" : "",
        ].filter(Boolean).join(" ");
        const startX = source.x + source.width / 2;
        const startY = source.y + source.height;
        const endX = target.x + target.width / 2;
        const endY = target.y;
        const midY = startY + (endY - startY) / 2;
        return `<path class="${className}" d="M ${startX} ${startY} C ${startX} ${midY}, ${endX} ${midY}, ${endX} ${endY}" />`;
    }

    function renderNode(node, layout) {
        const position = layout.positions.get(node.id);
        const selected = state.selectedModule;
        const outboundIds = linkedIds(selected, "outbound");
        const inboundIds = linkedIds(selected, "inbound");
        const className = [
            "map-node",
            selected === node.id ? "is-selected" : "",
            outboundIds.has(node.id) ? "is-outbound" : "",
            inboundIds.has(node.id) ? "is-inbound" : "",
        ].filter(Boolean).join(" ");

        const metricValue = mapMetricValue(node);
        const fill = scoreFill(metricValue || 0, state.mapMetric);
        const label = shortenLabel(node.id);
        const score = formatNumber.format(metricValue || 0);
        const locality = localityForModule(node);
        const leverage = leverageForModule(node);
        const chips = [
            `Q ${Math.round(node.quality_risk ?? node.maintainability_risk ?? 0)}`,
            `M ${Math.round(node.maintainability_risk || 0)}`,
            `T ${Math.round(node.correctness_risk || 0)}`,
            `C ${Math.round(node.change_risk || 0)}`,
            `P ${Math.round(node.performance_risk || 0)}`,
            `A ${Math.round(node.architectural_risk || 0)}`,
            locality ? `L ${Math.round(localityRisk(locality))}` : "",
            leverage ? `V ${Math.round(leverageRisk(leverage))}` : "",
        ].filter(Boolean).join(" · ");

        return `<g class="${className}" data-id="${escapeHtml(node.id)}" transform="translate(${position.x} ${position.y})">
            <title>${escapeHtml(node.id)}</title>
            <rect width="${position.width}" height="${position.height}" rx="16" fill="${fill}"></rect>
            <foreignObject x="14" y="12" width="${position.width - 28}" height="${position.height - 24}">
                <div xmlns="http://www.w3.org/1999/xhtml" class="node-label">
                    <strong>${escapeHtml(label)}</strong>
                    <span>${escapeHtml(state.mapMetric)} ${escapeHtml(score)}</span>
                    <span>${escapeHtml(chips)}</span>
                </div>
            </foreignObject>
        </g>`;
    }

    function linkedIds(selected, direction) {
        if (!selected || !state.map?.graph?.edges) {
            return new Set();
        }
        const ids = state.map.graph.edges
            .map((edge) => edge.data)
            .filter((edge) => direction === "outbound" ? edge.source === selected : edge.target === selected)
            .map((edge) => direction === "outbound" ? edge.target : edge.source);
        return new Set(ids);
    }

    function mapMetricValue(node) {
        if (state.mapMetric === 'maintainability') return node.maintainability_risk;
        if (state.mapMetric === 'quality') return node.quality_risk ?? node.maintainability_risk;
        if (state.mapMetric === 'correctness') return node.correctness_risk;
        if (state.mapMetric === 'change') return node.change_risk;
        if (state.mapMetric === 'performance') return node.performance_risk;
        if (state.mapMetric === 'architectural') return node.architectural_risk;
        if (state.mapMetric === 'locality') {
            const locality = localityForModule(node);
            return locality ? localityRisk(locality) : node.non_locality_risk ?? node.locality_risk;
        }
        if (state.mapMetric === 'leverage') {
            const leverage = leverageForModule(node);
            return leverage ? leverageRisk(leverage) : node.leverage_risk;
        }
        if (state.mapMetric === 'churn') return node.churn;
        return node.total_score;
    }

    function scoreFill(score, metric) {
        let bad = 600;
        let warn = 300;
        if (metric === 'maintainability' || metric === 'architectural') { bad = 350; warn = 150; }
        else if (metric === 'quality') { bad = 350; warn = 150; }
        else if (metric === 'correctness') { bad = 120; warn = 60; }
        else if (metric === 'change') { bad = 200; warn = 80; }
        else if (metric === 'performance') { bad = 100; warn = 30; }
        else if (metric === 'locality') { bad = 30; warn = 15; }
        else if (metric === 'leverage') { bad = 40; warn = 20; }
        else if (metric === 'churn') { bad = 500; warn = 150; }

        if (score >= bad) return "#6b2a35";
        if (score >= warn) return "#5e4b25";
        return "#244638";
    }

    function shortenLabel(id) {
        const parts = id.split("::");
        if (parts.length <= 2) {
            return id;
        }
        return `${parts.at(-2)}::${parts.at(-1)}`;
    }

    function renderMapDetail(modules, edges) {
        const selected = modules.find((node) => node.id === state.selectedModule);
        if (!selected) {
            const getMetric = (node) => mapMetricValue(node);
            const top5 = [...modules].sort((a, b) => (getMetric(b) || 0) - (getMetric(a) || 0)).slice(0, 5);
            const top5Html = top5.map((n, i) => {
                return `<div class="detail-row"><strong>${i + 1}. ${escapeHtml(shortenLabel(n.id))}</strong>${formatNumber.format(getMetric(n) || 0)}</div>`;
            }).join('');

            byId("map-detail").innerHTML = `<h2>Insights</h2>
                <p class="muted" style="margin-bottom: 1rem;">Top 5 modules by <strong>${state.mapMetric}</strong>. Click a module on the map to see details.</p>
                <div class="detail-list">${top5Html}</div>`;
            return;
        }

        const outbound = edges.filter((edge) => edge.source === selected.id).map((edge) => edge.target);
        const inbound = edges.filter((edge) => edge.target === selected.id).map((edge) => edge.source);
        const perf = selected.perf_benchmarks || [];
        const evidence = selected.evidence || {};
        const categorySignals = selected.category_signals || {};
        const locality = localityForModule(selected);
        const leverage = leverageForModule(selected);

        byId("map-detail").innerHTML = `<h2>${escapeHtml(selected.id)}</h2>
            <div class="detail-list">
                <div class="detail-row"><strong>Total risk</strong>${formatNumber.format(selected.total_score || 0)}</div>
                <div class="detail-row"><strong>Quality risk</strong>${formatNumber.format(selected.quality_risk ?? selected.maintainability_risk ?? 0)}</div>
                <div class="detail-row"><strong>Maintainability risk</strong>${formatNumber.format(selected.maintainability_risk || 0)}</div>
                <div class="detail-row"><strong>Correctness risk</strong>${formatNumber.format(selected.correctness_risk || 0)}</div>
                <div class="detail-row"><strong>Change risk</strong>${formatNumber.format(selected.change_risk || 0)}</div>
                <div class="detail-row"><strong>Performance risk</strong>${formatNumber.format(selected.performance_risk || 0)}</div>
                <div class="detail-row"><strong>Architectural risk</strong>${formatNumber.format(selected.architectural_risk || 0)}</div>
                <div class="detail-row"><strong>Locality risk</strong>${formatNumber.format(locality ? localityRisk(locality) : selected.non_locality_risk ?? selected.locality_risk ?? 0)}</div>
                <div class="detail-row"><strong>Leverage risk</strong>${formatNumber.format(leverage ? leverageRisk(leverage) : selected.leverage_risk ?? 0)}</div>
                <div class="detail-row"><strong>Lines of code</strong>${formatNumber.format(selected.sloc || 0)}</div>
                <div class="detail-row"><strong>Maintainability signals</strong>${renderPills(categorySignals.maintainability || [])}</div>
                <div class="detail-row"><strong>Change signals</strong>${renderPills(categorySignals.change || [])}</div>
                <div class="detail-row"><strong>Performance signals</strong>${renderPills(categorySignals.performance || [])}</div>
                <div class="detail-row"><strong>Correctness signals</strong>${renderPills(categorySignals.correctness || [])}</div>
                <div class="detail-row"><strong>Architectural signals</strong>${renderPills(categorySignals.architectural || [])}</div>
                <div class="detail-row"><strong>Public API</strong>${formatNumber.format(evidence.public_api_count || 0)}</div>
                <div class="detail-row"><strong>Commits / churn</strong>${formatNumber.format(evidence.commit_count || 0)} / ${formatNumber.format(evidence.churn || 0)}</div>
                <div class="detail-row"><strong>Contributors / defects</strong>${formatNumber.format(evidence.contributor_count || 0)} / ${formatNumber.format(evidence.defect_commits || 0)}</div>
                <div class="detail-row"><strong>Tests</strong>${evidence.has_tests ? "evidence found" : "no direct evidence"}${evidence.test_count != null ? ` (${formatNumber.format(evidence.test_count)})` : ""}</div>
                <div class="detail-row"><strong>Failed / unknown tests</strong>${formatNumber.format(evidence.failed_tests || 0)} / ${formatNumber.format(evidence.unknown_tests || 0)}</div>
                <div class="detail-row"><strong>Layer violations</strong>${formatNumber.format(evidence.layer_violations || 0)}</div>
                <div class="detail-row"><strong>Cycle member</strong>${evidence.cycle_member ? "yes" : "no"}</div>
                <div class="detail-row"><strong>Locality signals</strong>${renderPills(locality?.signals || [])}</div>
                <div class="detail-row"><strong>Leverage signals</strong>${renderPills(leverage?.signals || [])}</div>
                <div class="detail-row"><strong>Outbound dependencies</strong>${renderPills(outbound)}</div>
                <div class="detail-row"><strong>Inbound dependencies</strong>${renderPills(inbound)}</div>
                <div class="detail-row"><strong>Benchmarks</strong>${perf.length ? perf.map(renderBenchmark).join("") : '<span class="muted">-</span>'}</div>
            </div>`;
    }

    function renderBenchmark(item) {
        const dispersionLabel = item.dispersion_label || "median_abs_dev";
        const dispersion = item.dispersion_ms == null ? "-" : `${formatNumber.format(item.dispersion_ms)} ms ${dispersionLabel}`;
        return `<div class="pill">${escapeHtml(item.name)}: ${formatNumber.format(item.mean_ms)} ms mean, ${dispersion}</div>`;
    }

    function scenarioFlamegraphs(scenario) {
        const filters = performanceBucketFilters(scenario.id);
        const profileRows = filterScenarioEvidenceRows(scenario.evidence?.profiles || [], filters.profiles);
        const profileIds = new Set(profileRows.flatMap((item) => [item.id, item.name]).filter(Boolean));
        if (!profileIds.size) return [];
        return (state.flamegraphs || []).filter((item) => profileIds.has(item.id) || profileIds.has(item.name));
    }

    function renderScenarioFlamegraphs(scenario) {
        const list = document.querySelector(`[data-flamegraph-list="${CSS.escape(scenario.id)}"]`);
        const content = document.querySelector(`[data-flamegraph-content="${CSS.escape(scenario.id)}"]`);
        if (!list || !content) return;

        if (!state.flamegraphs || !state.flamegraphs.length) {
            list.innerHTML = '<p class="muted">No flamegraphs loaded.</p>';
            content.innerHTML = '<p class="muted">Generate flamegraphs using <code>open-overview.ps1 -Flamegraph</code> in an Administrator terminal.</p>';
            return;
        }

        const flamegraphs = scenarioFlamegraphs(scenario);
        if (!flamegraphs.length) {
            list.innerHTML = '<p class="muted">No flamegraphs match this promise.</p>';
            content.innerHTML = '<p class="muted">No matching profile SVG is listed for this promise.</p>';
            return;
        }

        if (!flamegraphs.some((item) => item.id === state.selectedFlamegraphsByScenario[scenario.id])) {
            state.selectedFlamegraphsByScenario[scenario.id] = flamegraphs[0].id;
        }
        const selectedId = state.selectedFlamegraphsByScenario[scenario.id];
        const selected = flamegraphs.find((item) => item.id === selectedId) || flamegraphs[0];

        list.innerHTML = flamegraphs.map((item) => {
            const isActive = selected.id === item.id;
            const isMissing = !item.available;
            return `<button type="button" class="flamegraph-item ${isActive ? 'is-active' : ''} ${isMissing ? 'is-error' : ''}" data-flamegraph-id="${escapeHtml(item.id)}">
                <h3>${escapeHtml(item.name)}</h3>
                <p>${escapeHtml(isMissing ? (item.issue || "Not generated") : item.id)}</p>
            </button>`;
        }).join("");

        loadFlamegraphInto(content, selected);
    }

    async function loadFlamegraphInto(content, selected) {
        if (!selected.available) {
            content.innerHTML = `<div class="flamegraph-error">
                <h3>${escapeHtml(selected.name)}</h3>
                <p>${escapeHtml(selected.issue || selected.description || "No SVG is currently available for this profile.")}</p>
                <p>${escapeHtml((selected.workload_families || []).join(", ") || "-")}</p>
                <p>${escapeHtml((selected.benchmark_keys || []).join(", ") || "-")}</p>
            </div>`;
            return;
        }

        content.innerHTML = '<p class="muted">Loading SVG...</p>';
        try {
            // Path in JSON is relative to repo root, but we serve from repo root.
            // Viewer is at /viewer/, so path should be /target/analysis/flamegraphs/x.svg
            // Or relative: ../target/analysis/flamegraphs/x.svg
            const svgPath = `../target/analysis/${selected.path}?v=${viewerVersion}`;
            const response = await fetch(svgPath);
            if (!response.ok) throw new Error(`HTTP ${response.status}`);
            const svgText = await response.text();

            // To make the SVG interactive and fit properly, we might need to strip 
            // explicit width/height or wrap it.
            content.innerHTML = svgText;
        } catch (e) {
            content.innerHTML = `<div class="flamegraph-error">
                <h3>Failed to load SVG</h3>
                <p>${escapeHtml(e.message)}</p>
                <p>Ensure the file exists at <code>target/analysis/${escapeHtml(selected.path)}</code></p>
            </div>`;
        }
    }

    async function loadRunLog(runId, targetId = "run-log-output") {
        if (!runId) return;
        state.selectedRun = runId;
        renderSelectedRunProgress();
        const output = byId(targetId);
        if (!output) return;
        if (targetId === "overview-run-log") output.hidden = false;
        output.textContent = "Loading run log...";
        try {
            const response = await fetch(`/api/run/${encodeURIComponent(runId)}/log`, { cache: "no-store" });
            if (!response.ok) throw new Error(`HTTP ${response.status}`);
            output.textContent = await response.text();
        } catch (error) {
            output.textContent = `No log available from the dashboard server.\n${error.message}`;
        }
    }

    async function refreshAppPackage() {
        const button = byId("app-package-refresh");
        const label = button?.dataset.runLabel || button?.textContent.trim() || "Refresh Package";
        if (button) {
            setButtonProgress(button, {
                label,
                meta: "Working",
                percent: 35,
                task: "app package",
                description: "Refreshing app package data",
            });
        }
        try {
            state.appPackage = await loadJson(`/api/app-package?v=${Date.now()}`);
            if (button) {
                setButtonProgress(button, {
                    label,
                    meta: "100%",
                    percent: 100,
                    task: "app package",
                    description: "App package data refreshed",
                });
            }
            byId("load-status").textContent = "Loaded app package.";
            byId("load-detail").textContent = `Session root: ${state.appPackage.session_root || "unknown"}`;
        } catch (error) {
            state.appPackage = null;
            byId("load-status").textContent = "App package unavailable.";
            byId("load-detail").textContent = `Start with scripts/open-overview.ps1 to enable the local dashboard API. ${error.message}`;
        } finally {
            if (button) clearButtonProgress(button, label);
            renderAppPackage();
        }
    }

    async function clearAppPackageBuffers() {
        const button = byId("app-package-clear-buffers");
        if (!window.confirm("Delete all persisted session tabs and buffer snapshot data, including dirty unsaved buffers?")) {
            return;
        }
        if (button) button.disabled = true;
        try {
            const response = await fetch("/api/app-package/clear-buffers", { method: "POST", cache: "no-store" });
            const payload = await response.json();
            state.appPackage = payload;
            const result = state.appPackage?.clear_result || {};
            byId("load-status").textContent = result.blocked
                ? (result.message || "Close Scratchpad before clearing buffers.")
                : `Cleared ${formatNumber.format(result.buffers_removed || 0)} buffers across ${formatNumber.format(result.tabs_removed || 0)} tabs.`;
            byId("load-detail").textContent = `Session root: ${state.appPackage.session_root || "unknown"}`;
            if (!result.blocked && result.dirty_buffers_removed) {
                byId("load-detail").textContent += ` Dirty buffers cleared: ${formatNumber.format(result.dirty_buffers_removed)}.`;
            }
            if (!response.ok && !result.blocked) {
                throw new Error(result.message || `Clear buffers returned ${response.status}`);
            }
        } catch (error) {
            byId("load-status").textContent = "Could not clear app package buffers.";
            byId("load-detail").textContent = error.message;
        } finally {
            if (button) button.disabled = false;
            renderAppPackage();
        }
    }

    async function refreshRuns() {
        try {
            const previousFinished = state.lastObservedFinishedRun;
            state.runs = await loadJson(`/api/runs?v=${Date.now()}`);
            renderRunButtonsProgress();
            const latestFinished = [...state.runs].reverse().find((item) => item.finished_at);
            if (latestFinished && latestFinished.id !== previousFinished) {
                state.lastObservedFinishedRun = latestFinished.id;
                await loadDefaults();
                return;
            }
            renderOverview();
            renderRunLog();
        } catch {
            renderRunLog();
        }
    }

    async function triggerRun(endpoint, button) {
        const label = button.dataset.runLabel || button.textContent.trim() || "Refresh";
        button.dataset.runLabel = label;
        setButtonProgress(button, {
            label,
            meta: "Queued",
            percent: 0,
            task: "",
            description: "Refresh queued",
        });
        try {
            const response = await fetch(endpoint, { method: "POST" });
            if (!response.ok) {
                const payload = await response.json().catch(() => null);
                if (response.status === 409 && payload) {
                    byId("load-status").textContent = "Refresh already running.";
                    byId("load-detail").textContent = payload.active_run_id
                        ? `Waiting for ${payload.active_run_id} to finish.`
                        : "Wait for the current dashboard refresh to finish.";
                    await refreshRuns();
                    return;
                }
                throw new Error(`HTTP ${response.status}`);
            }
            const payload = await response.json();
            byId("load-status").textContent = `Queued ${payload.run_id}.`;
            byId("load-detail").textContent = "Refresh is running through the local dashboard server.";
            await refreshRuns();
        } catch (error) {
            byId("load-status").textContent = "Dashboard refresh unavailable.";
            byId("load-detail").textContent = `Start with scripts/open-overview.ps1 to enable refresh controls. ${error.message}`;
        } finally {
            renderRunButtonsProgress();
        }
    }

    async function loadJson(url, options = {}) {
        const response = await fetch(url, { cache: "no-store", ...options });
        if (!response.ok) {
            throw new Error(`${url} returned ${response.status}`);
        }
        return response.json();
    }

    async function loadDefaults() {
        const status = byId("load-status");
        const detail = byId("load-detail");
        const keys = ["catalog", "runs", "hotspots", "slowspots", "searchSpeed", "capacityReport", "resourceProfiles", "speedReport", "performanceReview", "clones", "typeHealth", "escapeHatches", "locality", "leverage", "map", "projectCodeMetrics", "flamegraphs", "correctness", "appPackage"];
        const fallbacks = {
            catalog: null,
            runs: [],
            hotspots: [],
            slowspots: [],
            searchSpeed: [],
            capacityReport: null,
            resourceProfiles: null,
            speedReport: null,
            performanceReview: null,
            clones: [],
            typeHealth: [],
            escapeHatches: [],
            locality: [],
            leverage: [],
            map: null,
            projectCodeMetrics: null,
            flamegraphs: [],
            correctness: null,
            appPackage: null,
        };

        const settled = await Promise.allSettled(keys.map((key) => loadJson(sources[key])));
        const loaded = [];
        const missing = [];

        settled.forEach((result, index) => {
            const key = keys[index];
            if (result.status === "fulfilled") {
                state[key] = result.value;
                loaded.push(key);
            } else {
                state[key] = fallbacks[key];
                // flamegraphs is often missing if not generated, so we don't treat it as a loud error
                if (key !== "flamegraphs" && key !== "runs" && key !== "catalog" && key !== "appPackage") {
                    missing.push(`${key}: ${result.reason.message}`);
                }
            }
        });

        if (missing.length === 0) {
            status.textContent = "Loaded default JSON artifacts.";
            detail.textContent = "Data came from target/analysis.";
        } else if (loaded.length > 0) {
            status.textContent = `Loaded ${loaded.length} default artifact sets.`;
            detail.textContent = `Some default files were missing: ${missing.join("; ")}. Use Refresh to regenerate them.`;
        } else {
            status.textContent = "No artifacts loaded.";
            detail.textContent = `Default fetch failed: ${missing.join("; ")}. Start with scripts/open-overview.ps1 and use Refresh to regenerate artifacts.`;
        }
        renderAll();
    }

    function setupTabs() {
        document.querySelectorAll(".tab").forEach((button) => {
            button.addEventListener("click", () => {
                activateTab(button.dataset.tab);
            });
        });
    }

    function activateTab(tabId) {
        const button = document.querySelector(`.tab[data-tab="${CSS.escape(tabId)}"]`);
        const panel = byId(tabId);
        if (!button || !panel) return;
        document.querySelectorAll(".tab").forEach((tab) => tab.classList.remove("is-active"));
        document.querySelectorAll(".tab-panel").forEach((item) => item.classList.remove("is-active"));
        button.classList.add("is-active");
        panel.classList.add("is-active");
        if (tabId === "app-package") {
            renderAppPackage();
        }
    }

    function initialTabFromLocation() {
        const params = new URLSearchParams(window.location.search);
        const hash = window.location.hash.replace(/^#/, "");
        const requested = params.get("tab") || hash.split("?")[0];
        return requested || "overview";
    }

    function applyQualityHashState() {
        const hash = window.location.hash.replace(/^#/, "");
        const [, query = ""] = hash.split("?");
        if (!query) return;
        const params = new URLSearchParams(query);
        const dataset = params.get("qualityDataset");
        if (dataset) {
            state.qualityDatasetView = dataset;
        }
        [
            ["hotspots", "hotspots-filter"],
            ["clones", "clones-filter"],
            ["typeHealth", "type-health-filter"],
            ["escapeHatches", "escape-hatches-filter"],
            ["locality", "locality-filter"],
            ["leverage", "leverage-filter"],
        ].forEach(([key, id]) => {
            const value = params.get(`${key}Filter`);
            const input = byId(id);
            if (value != null && input) {
                input.value = value;
            }
        });
    }

    function updateQualityHash() {
        if (!byId("quality-review")?.classList.contains("is-active")) return;
        const params = new URLSearchParams();
        params.set("qualityDataset", state.qualityDatasetView);
        [
            ["hotspots", "hotspots-filter"],
            ["clones", "clones-filter"],
            ["typeHealth", "type-health-filter"],
            ["escapeHatches", "escape-hatches-filter"],
            ["locality", "locality-filter"],
            ["leverage", "leverage-filter"],
        ].forEach(([key, id]) => {
            const value = byId(id)?.value || "";
            if (value) params.set(`${key}Filter`, value);
        });
        const next = `#quality-review?${params.toString()}`;
        if (window.location.hash !== next) {
            history.replaceState(null, "", next);
        }
    }

    function renderAll() {
        renderOverview();
        renderHotspots();
        renderQualityOverview();
        renderTypeHealthScatter();
        renderQualityDistribution();
        renderTypeHealthDistribution();
        renderEscapeHatchDistribution();
        renderCloneDistribution();
        renderPerformanceReviewCoverage();
        renderPerformanceOverview();
        renderPerformanceFilterOptions();
        renderPerformanceHeadlineCharts();
        renderPerformanceCuratedLists();
        renderPerformanceMeasurementGaps();
        renderClones();
        renderTypeHealth();
        renderEscapeHatches();
        renderCorrectness();
        renderCorrectnessMatrix();
        renderLocalityLeverage();
        renderMap();
        renderAppPackage();
        renderRunLog();
        renderRunButtonsProgress();
    }

    function renderAppPackage() {
        const payload = state.appPackage;
        const summaryTarget = byId("app-package-summary");
        if (!summaryTarget) return;
        if (!payload) {
            renderSummary("app-package-summary", [
                metricCard("Status", "Unavailable"),
                metricCard("Tabs", "-"),
                metricCard("Buffers", "-"),
                metricCard("Diagnostics", "-"),
            ]);
            byId("app-package-root").innerHTML = `<p class="muted">No app package payload loaded.</p>`;
            renderAppPackageInsights(null);
            updateAppPackageCount("app-package-diagnostics-count", 0, 0, "events");
            updateAppPackageCount("app-package-buffers-count", 0, 0, "buffers");
            updateAppPackageCount("app-package-topology-count", 0, 0, "tabs");
            updateAppPackageCount("app-package-files-count", 0, 0, "files");
            renderTable("app-package-buffers", ["Buffer", "Path", "Encoding", "Snapshot", "Dirty"], []);
            renderTable("app-package-topology", ["Tab", "Active View", "Views", "Root Pane"], []);
            renderTable("app-package-diagnostics", ["Line", "Kind", "Operation", "Source", "Message"], []);
            renderTable("app-package-files", ["Kind", "Name", "Path", "Size", "Modified", "Status"], []);
            byId("app-package-manifest").textContent = "No app package payload loaded.";
            byId("app-package-warnings").innerHTML = `<p class="muted">No loader warnings.</p>`;
            renderAppPackageDataView();
            return;
        }

        const summary = payload.manifest_summary || {};
        renderSummary("app-package-summary", [
            metricCard("Status", payload.exists ? "Found" : "Missing"),
            metricCard("Tabs", summary.tab_count ?? 0),
            metricCard("Buffers", summary.buffer_count ?? 0),
            metricCard("Diagnostics", summary.diagnostic_count ?? 0),
        ]);

        byId("app-package-root").innerHTML = `
            <div class="package-paths__row"><span>Session root</span><code>${escapeHtml(payload.session_root || "-")}</code></div>
            <div class="package-paths__row"><span>Manifest</span><code>${escapeHtml(payload.manifest_path || "-")}</code></div>
            <div class="package-paths__row"><span>Manifest file</span>${appPackageFileMeta(payload.manifest_file)}</div>
            <div class="package-paths__row"><span>Error log</span><code>${escapeHtml(payload.error_log_path || "-")}</code></div>
            <div class="package-paths__row"><span>Error log file</span>${appPackageFileMeta(payload.error_log_file)}</div>
        `;

        renderAppPackageInsights(payload);
        renderAppPackageBuffers(payload.buffers || []);
        renderAppPackageTopology(payload.topology || []);
        renderAppPackageDiagnostics(payload.diagnostics || []);
        renderAppPackageFiles(payload);
        renderAppPackageManifest(payload.manifest);
        renderAppPackageDataView();
        const warnings = payload.warnings || [];
        byId("app-package-warnings").innerHTML = warnings.length
            ? `<ul class="warning-list">${warnings.map((warning) => `<li>${escapeHtml(warning)}</li>`).join("")}</ul>`
            : `<p class="muted">No loader warnings.</p>`;
    }

    function renderAppPackageInsights(payload) {
        const target = byId("app-package-insights");
        if (!target) return;
        if (!payload) {
            target.innerHTML = `<section class="app-package-insight-card">
                <div class="app-package-insight-card__header">
                    <span class="app-package-insight-card__marker"></span>
                    <div>
                        <h3>Package health</h3>
                        <p>No app package payload loaded.</p>
                    </div>
                </div>
                <div class="app-package-empty-graph">Start with the dashboard API to inspect runtime state.</div>
            </section>`;
            return;
        }

        const buffers = payload.buffers || [];
        const topology = payload.topology || [];
        const diagnostics = payload.diagnostics || [];
        const snapshotOk = buffers.filter((buffer) => buffer.snapshot?.exists).length;
        const snapshotMissing = Math.max(0, buffers.length - snapshotOk);
        const dirtyBuffers = buffers.filter((buffer) => buffer.is_dirty).length;
        const cleanBuffers = Math.max(0, buffers.length - dirtyBuffers);
        const totalSnapshotBytes = buffers.reduce((sum, buffer) => sum + Number(buffer.snapshot?.size_bytes || 0), 0);
        const summary = payload.manifest_summary || {};
        const manifestBytes = Number(payload.manifest_file?.size_bytes || 0);
        const errorLogBytes = Number(payload.error_log_file?.size_bytes || 0);

        target.innerHTML = `
            <section class="app-package-insight-card app-package-insight-card--health">
                <div class="app-package-insight-card__header">
                    <span class="app-package-insight-card__marker"></span>
                    <div>
                        <h3>Snapshot health</h3>
                        <p>${formatNumber.format(snapshotOk)} available / ${formatNumber.format(snapshotMissing)} missing</p>
                    </div>
                </div>
                ${appPackageDonutChart([
                    { label: "Available", value: snapshotOk, cls: "good" },
                    { label: "Missing", value: snapshotMissing, cls: "bad" },
                ], buffers.length, snapshotOk, "available")}
                <div class="app-package-insight-card__metrics">
                    <span><strong>${escapeHtml(formatBytes(totalSnapshotBytes))}</strong> snapshot bytes</span>
                    <span><strong>${formatNumber.format(payload.buffer_files?.length || 0)}</strong> temp files</span>
                </div>
            </section>
            <section class="app-package-insight-card app-package-insight-card--dirty">
                <div class="app-package-insight-card__header">
                    <span class="app-package-insight-card__marker"></span>
                    <div>
                        <h3>Buffer state</h3>
                        <p>${formatNumber.format(dirtyBuffers)} dirty / ${formatNumber.format(cleanBuffers)} clean</p>
                    </div>
                </div>
                ${appPackageDonutChart([
                    { label: "Clean", value: cleanBuffers, cls: "good" },
                    { label: "Dirty", value: dirtyBuffers, cls: "warn" },
                ], buffers.length, cleanBuffers, "clean")}
                <div class="app-package-insight-card__metrics">
                    <span><strong>${formatNumber.format(buffers.length)}</strong> buffers</span>
                    <span><strong>${formatNumber.format(topology.length)}</strong> tabs</span>
                    <span><strong>${formatNumber.format(diagnostics.length)}</strong> diagnostics</span>
                </div>
            </section>
            <section class="app-package-insight-card app-package-insight-card--shape">
                <div class="app-package-insight-card__header">
                    <span class="app-package-insight-card__marker"></span>
                    <div>
                        <h3>Runtime shape</h3>
                        <p>Session object counts at a glance.</p>
                    </div>
                </div>
                ${appPackageColumnChart([
                    { label: "Tabs", value: summary.tab_count ?? topology.length },
                    { label: "Buffers", value: summary.buffer_count ?? buffers.length },
                    { label: "Views", value: summary.view_count ?? 0 },
                    { label: "Events", value: summary.diagnostic_count ?? diagnostics.length },
                ])}
            </section>
            <section class="app-package-insight-card app-package-insight-card--storage">
                <div class="app-package-insight-card__header">
                    <span class="app-package-insight-card__marker"></span>
                    <div>
                        <h3>Storage mix</h3>
                        <p>Manifest, log, and snapshot bytes.</p>
                    </div>
                </div>
                ${appPackageColumnChart([
                    { label: "Manifest", value: manifestBytes, display: formatBytes(manifestBytes) },
                    { label: "Log", value: errorLogBytes, display: formatBytes(errorLogBytes) },
                    { label: "Snapshots", value: totalSnapshotBytes, display: formatBytes(totalSnapshotBytes) },
                ])}
                <div class="app-package-insight-card__metrics">
                    <span><strong>${formatNumber.format(payload.buffer_files?.length || 0)}</strong> snapshot files</span>
                    <span><strong>${escapeHtml(formatBytes(manifestBytes + errorLogBytes + totalSnapshotBytes))}</strong> total</span>
                </div>
            </section>
        `;
    }

    function appPackageDonutChart(segments, total, centerValue, centerLabel) {
        const denominator = Math.max(total, 1);
        const radius = 42;
        const circumference = 2 * Math.PI * radius;
        let offset = 0;
        const rings = segments.map((segment) => {
            const length = (segment.value / denominator) * circumference;
            const ring = `<circle class="app-package-donut__segment app-package-donut__segment--${segment.cls}" cx="58" cy="58" r="${radius}" stroke-dasharray="${length} ${Math.max(0, circumference - length)}" stroke-dashoffset="${-offset}"></circle>`;
            offset += length;
            return ring;
        }).join("");
        return `<div class="app-package-donut">
            <svg viewBox="0 0 116 116" role="img" aria-label="${escapeHtml(centerLabel)} ${formatNumber.format(centerValue)} of ${formatNumber.format(total)}">
                <circle class="app-package-donut__track" cx="58" cy="58" r="${radius}"></circle>
                ${rings}
                <text x="58" y="54" class="app-package-donut__value">${escapeHtml(centerValue)}</text>
                <text x="58" y="72" class="app-package-donut__label">${escapeHtml(centerLabel)}</text>
            </svg>
        </div>
        <div class="app-package-segment-legend">
            ${segments.map((segment) => `<span><i class="app-package-legend-swatch app-package-legend-swatch--${segment.cls}"></i>${escapeHtml(segment.label)} ${formatNumber.format(segment.value)}</span>`).join("")}
        </div>`;
    }

    function appPackageColumnChart(items) {
        const maxValue = Math.max(...items.map((item) => Number(item.value || 0)), 1);
        return `<div class="app-package-column-chart">
            ${items.map((item) => {
                const value = Number(item.value || 0);
                const height = Math.max(4, (value / maxValue) * 100);
                return `<div class="app-package-column-chart__item">
                    <div class="app-package-column-chart__plot">
                        <span style="height:${height}%"></span>
                    </div>
                    <strong>${escapeHtml(item.display ?? formatNumber.format(value))}</strong>
                    <em>${escapeHtml(item.label)}</em>
                </div>`;
            }).join("")}
        </div>`;
    }

    function renderAppPackageDataView() {
        document.querySelectorAll("[data-app-package-view]").forEach((button) => {
            const active = button.dataset.appPackageView === state.appPackageView;
            button.classList.toggle("is-active", active);
            button.setAttribute("aria-pressed", active ? "true" : "false");
        });
        document.querySelectorAll("[data-app-package-panel]").forEach((panel) => {
            panel.classList.toggle("is-active", panel.dataset.appPackagePanel === state.appPackageView);
        });
    }

    function updateAppPackageCount(targetId, visible, total, noun) {
        const target = byId(targetId);
        if (!target) return;
        target.textContent = visible === total
            ? `${formatNumber.format(total)} ${noun}`
            : `${formatNumber.format(visible)} of ${formatNumber.format(total)} ${noun}`;
    }

    function appPackageFileMeta(file) {
        if (!file || !file.exists) {
            return `<span class="risk-bad">missing</span>`;
        }
        const size = formatBytes(file.size_bytes || 0);
        const modified = file.modified_at == null
            ? ""
            : `<span class="muted">${escapeHtml(new Date(file.modified_at * 1000).toLocaleString())}</span>`;
        return `<span class="package-file-meta"><span class="risk-good">${escapeHtml(size)}</span>${modified}</span>`;
    }

    function renderAppPackageBuffers(buffers) {
        const query = byId("app-package-buffer-filter")?.value || "";
        const filtered = buffers.filter((buffer) => matchesFilter(buffer, query));
        updateAppPackageCount("app-package-buffers-count", filtered.length, buffers.length, "buffers");
        renderTable(
            "app-package-buffers",
            ["Tab", "Buffer", "Path", "Encoding", "Snapshot", "History", "Dirty"],
            filtered.map((buffer) => {
                const snapshot = buffer.snapshot || {};
                const snapshotText = snapshot.exists
                    ? `${formatBytes(snapshot.size_bytes || 0)} / ${escapeHtml(snapshot.name || "-")}`
                    : "missing";
                return `<tr class="${buffer.is_dirty ? "app-package-row--dirty" : ""}">
                    <td>${Number(buffer.tab_index || 0) + 1}</td>
                    <td><code>${escapeHtml(buffer.name || "Untitled")}</code><div class="muted">id ${escapeHtml(buffer.id ?? "-")} / temp ${escapeHtml(buffer.temp_id || "-")}</div></td>
                    <td><code>${escapeHtml(buffer.path || "unsaved")}</code></td>
                    <td>${escapeHtml(buffer.encoding || "-")}<div class="muted">BOM ${buffer.has_bom ? "yes" : "no"}</div></td>
                    <td class="${snapshot.exists ? "risk-good" : "risk-bad"}">${snapshotText}</td>
                    <td>${formatNumber.format(buffer.text_history_count || 0)}</td>
                    <td>${buffer.is_dirty ? `<span class="risk-warn">dirty</span>` : `<span class="muted">clean</span>`}</td>
                </tr>`;
            })
        );
    }

    function renderAppPackageTopology(topology) {
        updateAppPackageCount("app-package-topology-count", topology.length, topology.length, "tabs");
        renderTable(
            "app-package-topology",
            ["Tab", "Active View", "Views", "Root Pane"],
            topology.map((tab) => `<tr>
                <td>${Number(tab.tab_index || 0) + 1}</td>
                <td><code>${escapeHtml(tab.active_view_id ?? "-")}</code></td>
                <td>${formatNumber.format(tab.view_count || 0)}<div class="muted">${escapeHtml((tab.view_ids || []).join(", ") || "-")}</div></td>
                <td><span class="app-package-token">${escapeHtml(tab.root_pane_kind || "unknown")}</span></td>
            </tr>`)
        );
    }

    function renderAppPackageDiagnostics(diagnostics) {
        const query = byId("app-package-diagnostics-filter")?.value || "";
        const filtered = diagnostics.filter((item) => matchesFilter(item, query)).slice().reverse();
        updateAppPackageCount("app-package-diagnostics-count", filtered.length, diagnostics.length, "events");
        renderTable(
            "app-package-diagnostics",
            ["Line", "Kind", "Operation", "Source", "Message", "Details"],
            filtered.map((item) => {
                const details = item.details && typeof item.details === "object"
                    ? Object.entries(item.details)
                        .map(([key, value]) => `${key}=${typeof value === "object" ? JSON.stringify(value) : value}`)
                        .join("; ")
                    : "";
                return `<tr>
                    <td>${escapeHtml(item.line ?? "-")}</td>
                    <td><span class="app-package-token">${escapeHtml(item.kind || "-")}</span></td>
                    <td>${escapeHtml(item.operation || "-")}</td>
                    <td><code>${escapeHtml(item.source || "-")}</code></td>
                    <td>${escapeHtml(item.message || "-")}<div class="muted">${escapeHtml(item.path || "")}</div></td>
                    <td class="small-text">${escapeHtml(details)}</td>
                </tr>`;
            })
        );
    }

    function renderAppPackageFiles(payload) {
        const files = [
            { kind: "manifest", ...(payload.manifest_file || {}) },
            { kind: "error log", ...(payload.error_log_file || {}) },
            ...(payload.buffer_files || []).map((file) => ({ kind: "snapshot", ...file })),
        ];
        updateAppPackageCount("app-package-files-count", files.length, files.length, "files");
        renderTable(
            "app-package-files",
            ["Kind", "Name", "Path", "Size", "Modified", "Status"],
            files.map((file) => `<tr>
                <td><span class="app-package-token">${escapeHtml(file.kind || "-")}</span></td>
                <td><code>${escapeHtml(file.name || "-")}</code></td>
                <td><code>${escapeHtml(file.path || "-")}</code></td>
                <td>${escapeHtml(file.exists ? formatBytes(file.size_bytes || 0) : "-")}</td>
                <td>${escapeHtml(file.modified_at == null ? "-" : new Date(file.modified_at * 1000).toLocaleString())}</td>
                <td class="${file.exists ? "risk-good" : "risk-bad"}">${escapeHtml(file.exists ? "available" : file.error || "missing")}</td>
            </tr>`)
        );
    }

    function renderAppPackageManifest(manifest) {
        const target = byId("app-package-manifest");
        if (!target) return;
        target.textContent = manifest ? JSON.stringify(manifest, null, 2) : "No manifest loaded.";
    }

    function renderQualityDistribution() {
        const target = byId("quality-distribution");
        if (!target) return;
        const thresholds = empiricalThresholds(qualityDistributionItems().map((item) => item.score), 300, 600);
        renderRiskDistribution(target, qualityDistributionItems(), {
            empty: "No hotspot data.",
            modeKey: "qualityDistributionMode",
            expandedKey: "expandedQualityKey",
            warn: thresholds.warn,
            bad: thresholds.bad,
            scoreLabel: "quality score",
            dataset: "hotspots",
            filterField: "quality",
        });
    }

    function renderCloneDistribution() {
        const target = byId("clone-distribution");
        if (!target) return;
        renderRiskDistribution(target, cloneDistributionItems(), {
            empty: "No clone data.",
            modeKey: "cloneDistributionMode",
            expandedKey: "expandedCloneKey",
            warn: 20,
            bad: 40,
            scoreLabel: "clone score",
            dataset: "clones",
            filterField: "score",
        });
    }

    function renderTypeHealthDistribution() {
        const target = byId("type-health-distribution");
        if (!target) return;
        renderRiskDistribution(target, typeHealthDistributionItems(), {
            empty: "No type health data.",
            modeKey: "typeHealthDistributionMode",
            expandedKey: "expandedTypeHealthKey",
            warn: 40,
            bad: 70,
            scoreLabel: "structural risk",
            minScore: 8,
            filteredOutLabel: "stable low-risk types",
            dataset: "typeHealth",
            filterField: "risk",
        });
    }

    function renderEscapeHatchDistribution() {
        const target = byId("escape-hatches-distribution");
        if (!target) return;
        renderRiskDistribution(target, escapeHatchDistributionItems(), {
            empty: "No escape hatch data.",
            modeKey: "escapeHatchDistributionMode",
            expandedKey: "expandedEscapeHatchKey",
            warn: 20,
            bad: 50,
            scoreLabel: "escape hatch score",
            dataset: "escapeHatches",
            filterField: "score",
        });
    }

    function qualityDistributionItems() {
        return (state.hotspots || []).map((h) => ({
            key: `hotspot:${h.name}:${h.start_line || ""}`,
            kind: "hotspot",
            name: h.name,
            score: qualityScore(h),
            signals: h.signals || [],
            details: `${formatNumber.format(h.sloc || 0)} SLOC`,
            raw: h,
            searchText: [h.kind, h.name, h.signals, h.sloc, qualityScore(h)].join(" "),
        }));
    }

    function cloneDistributionItems() {
        return (state.clones || []).map((c) => {
            const hash = c.hash || c.group_hash || "";
            const instances = c.instances || [];
            return {
                key: `clone:${c.hash || c.group_hash || c.name || ""}`,
                kind: "clone",
                name: `clone ${hash.substring(0, 8) || "group"} (${c.instance_count || instances.length || 0}x)`,
                score: Number(c.score || 0),
                signals: c.signals || [],
                details: `${c.token_count || 0} tokens`,
                raw: c,
                searchText: [c.engine, hash, c.score, c.token_count, c.signals, ...instances.map((inst) => inst.file_path)].join(" "),
            };
        });
    }

    function typeHealthDistributionItems() {
        return (state.typeHealth || []).map((item) => ({
            key: `type:${item.qualified_name || item.type_name}`,
            kind: item.kind || "type",
            name: item.qualified_name || item.type_name,
            score: typeHealthRisk(item),
            signals: item.signals || [],
            details: `${formatNumber.format(item.field_count || item.variant_count || 0)} width · ${formatNumber.format(item.method_count || 0)} methods · ${formatNumber.format(item.impl_file_count || 0)} files`,
            raw: item,
            searchText: [
                item.kind,
                item.type_name,
                item.qualified_name,
                item.path,
                item.module_key,
                item.signals,
                item.field_count,
                item.variant_count,
                item.method_count,
                item.impl_file_count,
            ].join(" "),
        }));
    }

    function escapeHatchDistributionItems() {
        return (state.escapeHatches || []).map((item) => ({
            key: `escape:${item.module_key || item.module_name}`,
            kind: "escape hatch",
            name: item.module_key || item.module_name,
            score: Number(item.escape_hatch_score || 0),
            signals: item.signals || [],
            details: `${formatNumber.format(item.total_count || 0)} uses · unsafe ${formatNumber.format(item.unsafe_count || 0)}`,
            raw: item,
            searchText: [item.module_key, item.module_name, item.path, item.signals, item.escape_hatch_score].join(" "),
        }));
    }

    function renderTypeHealthScatter() {
        const target = byId("type-health-scatter");
        if (!target) return;
        const rows = (state.typeHealth || []).filter((item) => {
            const width = Math.max(Number(item.field_count || 0), Number(item.variant_count || 0));
            return typeHealthRisk(item) >= 8
                || width >= 8
                || Number(item.method_count || 0) >= 12
                || Number(item.impl_file_count || 0) >= 2;
        });
        if (!rows.length) {
            target.innerHTML = `<p class="muted">No type health data.</p>`;
            return;
        }
        const width = 760;
        const height = 380;
        const margin = { left: 70, right: 28, top: 28, bottom: 60 };
        const plotWidth = width - margin.left - margin.right;
        const plotHeight = height - margin.top - margin.bottom;
        const typeWidth = (item) => Math.max(Number(item.field_count || 0), Number(item.variant_count || 0));
        const maxWidth = Math.max(20, ...rows.map(typeWidth));
        const maxMethods = Math.max(20, ...rows.map((item) => Number(item.method_count || 0)));
        const useRiskY = state.typeHealthScatterY === "risk";
        const maxY = useRiskY ? 100 : Math.log1p(maxMethods);
        const x = (value) => margin.left + (Math.max(0, Math.min(maxWidth, value)) / maxWidth) * plotWidth;
        const y = (value) => {
            if (useRiskY) return margin.top + (1 - (Math.max(0, Math.min(100, value)) / 100)) * plotHeight;
            return margin.top + (1 - (Math.log1p(Math.max(0, value)) / maxY)) * plotHeight;
        };
        const yValue = (item) => useRiskY ? typeHealthRisk(item) : Number(item.method_count || 0);
        const widthCut = x(16);
        const methodCut = y(useRiskY ? 40 : 20);
        const highRisk = [...rows].sort((a, b) => typeHealthRisk(b) - typeHealthRisk(a)).slice(0, 8);
        const highRiskKeys = new Set(highRisk.map((item) => item.qualified_name || item.type_name));
        const points = rows.map((item, index) => {
            const risk = typeHealthRisk(item);
            const cls = risk >= 40 ? "bad" : risk >= 25 ? "warn" : "good";
            const radius = Math.max(4, Math.min(13, 4 + Math.sqrt(Number(item.impl_file_count || 0) + 1) * 2));
            const key = item.qualified_name || item.type_name;
            const topClass = highRiskKeys.has(key) ? "is-top-risk" : "";
            const label = `${key}: risk ${formatNumber.format(risk)}, width ${formatNumber.format(typeWidth(item))}, methods ${formatNumber.format(item.method_count || 0)}, impl files ${formatNumber.format(item.impl_file_count || 0)}`;
            return `<circle class="type-health-point type-health-point--${cls} ${topClass}" cx="${x(typeWidth(item)).toFixed(1)}" cy="${y(yValue(item)).toFixed(1)}" r="${radius.toFixed(1)}" tabindex="0" role="button" data-type-health-index="${index}" aria-label="${escapeHtml(label)}">
                <title>${escapeHtml(label)}</title>
            </circle>`;
        }).join("");
        target.innerHTML = `<div class="type-health-scatter__layout">
            <svg class="type-health-scatter__plot" viewBox="0 0 ${width} ${height}" role="img" aria-label="Type width versus method surface scatter plot">
                <rect class="type-health-zone type-health-zone--watch" x="${widthCut}" y="${margin.top}" width="${margin.left + plotWidth - widthCut}" height="${methodCut - margin.top}"></rect>
                <line class="ll-threshold" x1="${widthCut}" x2="${widthCut}" y1="${margin.top}" y2="${margin.top + plotHeight}"></line>
                <line class="ll-threshold" x1="${margin.left}" x2="${margin.left + plotWidth}" y1="${methodCut}" y2="${methodCut}"></line>
                <line class="ll-axis" x1="${margin.left}" x2="${margin.left}" y1="${margin.top}" y2="${margin.top + plotHeight}"></line>
                <line class="ll-axis" x1="${margin.left}" x2="${margin.left + plotWidth}" y1="${margin.top + plotHeight}" y2="${margin.top + plotHeight}"></line>
                <text class="ll-axis-label" x="${margin.left + plotWidth / 2}" y="${height - 16}">Fields or enum variants</text>
                <text class="ll-axis-label ll-axis-label--y" x="20" y="${margin.top + plotHeight / 2}">${useRiskY ? "Structural risk" : "Method surface (log)"}</text>
                <text class="ll-tick" x="${x(0)}" y="${height - 38}">0</text>
                <text class="ll-tick" x="${x(16)}" y="${height - 38}">16</text>
                <text class="ll-tick" x="${x(maxWidth)}" y="${height - 38}">${formatNumber.format(maxWidth)}</text>
                <text class="ll-tick" x="${margin.left - 28}" y="${y(useRiskY ? 100 : maxMethods) + 4}">${formatNumber.format(useRiskY ? 100 : maxMethods)}</text>
                <text class="ll-tick" x="${margin.left - 22}" y="${y(useRiskY ? 40 : 20) + 4}">${useRiskY ? "40" : "20"}</text>
                <text class="ll-tick" x="${margin.left - 14}" y="${y(0) + 4}">0</text>
                ${points}
            </svg>
            <div class="ll-popover type-health-popover" hidden></div>
            <div class="ll-ranked-list">
                <h3>Structural tail</h3>
                ${highRisk.map((item, index) => `<button type="button" class="ll-ranked-row type-health-ranked-row" data-type-health-index="${rows.indexOf(item)}" title="${escapeHtml(item.qualified_name || item.type_name)}">
                    <span>${index + 1}</span>
                    <code>${escapeHtml(shortenLabel(item.qualified_name || item.type_name))}</code>
                    <strong>${formatNumber.format(typeHealthRisk(item))}</strong>
                </button>`).join("")}
            </div>
        </div>`;
        const popover = target.querySelector(".type-health-popover");
        const setActiveTypePoint = (index) => {
            target.querySelectorAll("[data-type-health-index]").forEach((element) => {
                element.classList.toggle("is-active", Number(element.dataset.typeHealthIndex) === index);
            });
        };
        const closeTypePopover = () => {
            if (popover) popover.hidden = true;
            setActiveTypePoint(-1);
        };
        const showTypePopover = (index) => {
            const item = rows[index];
            if (!item || !popover) return;
            setActiveTypePoint(index);
            const dotX = x(typeWidth(item));
            const dotY = y(yValue(item));
            const left = (dotX / width) * 100;
            const top = (dotY / height) * 100;
            const risk = typeHealthRisk(item);
            const riskLabel = risk >= 40 ? "high" : risk >= 25 ? "watch" : "lower";
            popover.hidden = false;
            popover.classList.toggle("ll-popover--left", left > 68);
            popover.classList.toggle("ll-popover--top", top < 24);
            popover.classList.toggle("ll-popover--bottom", top > 76);
            popover.style.left = `${left}%`;
            popover.style.top = `${top}%`;
            popover.innerHTML = `<strong>${escapeHtml(item.qualified_name || item.type_name)}</strong>
                <code>${escapeHtml(item.path || "")}:${escapeHtml(item.line || "-")}</code>
                <div><span>Dot X</span><b>${formatNumber.format(typeWidth(item))} fields/variants</b></div>
                <div><span>Dot Y</span><b>${useRiskY ? `${formatNumber.format(risk)} risk` : `${formatNumber.format(item.method_count || 0)} methods`}</b></div>
                <div><span>Dot size</span><b>${formatNumber.format(item.impl_file_count || 0)} impl files</b></div>
                <div><span>Dot color</span><b>${escapeHtml(riskLabel)} structural risk</b></div>
                <div><span>Risk</span><b>${formatNumber.format(risk)}</b></div>
                <div class="type-health-popover__signals">${renderPills(item.signals)}</div>`;
        };
        target.querySelectorAll(".type-health-point").forEach((point) => {
            point.addEventListener("click", (event) => {
                event.stopPropagation();
                showTypePopover(Number(point.dataset.typeHealthIndex));
            });
            point.addEventListener("keydown", (event) => {
                if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    showTypePopover(Number(point.dataset.typeHealthIndex));
                }
            });
        });
        target.querySelectorAll(".type-health-ranked-row").forEach((row) => {
            row.addEventListener("click", (event) => {
                event.stopPropagation();
                showTypePopover(Number(row.dataset.typeHealthIndex));
            });
        });
        target.querySelector(".type-health-scatter__layout")?.addEventListener("click", () => {
            closeTypePopover();
        });
        popover?.addEventListener("click", (event) => {
            event.stopPropagation();
        });
        if (!window.__scratchpadTypeHealthOutsideClickBound) {
            window.__scratchpadTypeHealthOutsideClickBound = true;
            document.addEventListener("click", (event) => {
                const activeScatter = document.querySelector("#type-health-scatter .type-health-scatter__layout");
                const activePopover = document.querySelector("#type-health-scatter .type-health-popover");
                if (!activeScatter || !activePopover || activePopover.hidden) return;
                if (activeScatter.contains(event.target)) return;
                activePopover.hidden = true;
                document.querySelectorAll("#type-health-scatter [data-type-health-index]").forEach((element) => {
                    element.classList.remove("is-active");
                });
            });
        }
    }

    function renderRiskDistribution(target, items, options) {
        const filteredOutCount = Number(options.minScore || 0) > 0
            ? items.filter((item) => Number(item.score || 0) < Number(options.minScore)).length
            : 0;
        if (Number(options.minScore || 0) > 0) {
            items = items.filter((item) => Number(item.score || 0) >= Number(options.minScore));
        }
        if (!items.length) {
            const filteredNote = filteredOutCount
                ? `<p class="muted">${formatNumber.format(filteredOutCount)} ${escapeHtml(options.filteredOutLabel || "low-risk items")} filtered from this tail view.</p>`
                : "";
            target.innerHTML = `<p class="muted">${escapeHtml(options.empty)}</p>${filteredNote}`;
            return;
        }
        const sorted = items.slice().sort((a, b) => b.score - a.score);
        const mode = state[options.modeKey] || "counts";
        const curve = riskDistributionCurve(sorted, options);
        const body = mode === "counts"
            ? renderRiskDistributionCounts(sorted, options)
            : renderDistributionRows(sorted.slice(0, 10), options, { ranked: true });
        const filteredNote = filteredOutCount
            ? `<p class="tail-filter-note">${formatNumber.format(filteredOutCount)} ${escapeHtml(options.filteredOutLabel || "low-risk items")} hidden from this tail view.</p>`
            : "";
        target.innerHTML = curve + filteredNote + body;
        attachPerformanceDistributionHandlers(target);
        target.querySelectorAll("[data-risk-key]").forEach((button) => {
            button.addEventListener("click", () => {
                state[options.expandedKey] = state[options.expandedKey] === button.dataset.riskKey ? null : button.dataset.riskKey;
                renderRiskDistribution(target, items, options);
            });
        });
    }

    function renderRiskDistributionCounts(items, options) {
        const buckets = riskBuckets(items, options.warn, options.bad);
        const signalCounts = new Map();
        items.forEach((item) => {
            pillValues(item.signals).forEach((sig) => {
                signalCounts.set(sig, (signalCounts.get(sig) || 0) + 1);
            });
        });
        const sigSorted = [...signalCounts.entries()].sort((a, b) => b[1] - a[1]).slice(0, 6);
        const sigMax = sigSorted.length ? sigSorted[0][1] : 1;
        return `<div class="risk-count-grid">
            <div class="risk-bucket-list">
                ${buckets.map((bucket) => `<div class="risk-bucket-list__item">
                    <span class="quality-pie__swatch quality-pie__swatch--${bucket.cls}"></span>
                    <span>${escapeHtml(bucket.label)}</span>
                    <strong>${formatNumber.format(bucket.value)}</strong>
                </div>`).join("")}
            </div>
            ${sigSorted.length ? `<div class="signal-bars">${sigSorted.map(([sig, count]) => `
                <div class="signal-bars__row">
                    <span>${escapeHtml(sig)}</span>
                    <div class="signal-bars__track"><span class="signal-bars__fill" style="width:${(count / sigMax) * 100}%"></span></div>
                    <span class="signal-bars__count">${formatNumber.format(count)}</span>
                </div>`).join("")}</div>` : `<p class="muted">No signal data.</p>`}
        </div>`;
    }

    function renderDistributionRows(items, options, flags = {}) {
        if (!items.length) {
            return `<p class="muted">No items match.</p>`;
        }
        return `<div class="quality-feed ${flags.ranked ? "quality-feed--ranked" : ""}">${items.map((it, index) => {
            const cls = riskClass(it.score, options.warn, options.bad);
            const tail = (it.name || "").split(/[\\/]/).pop();
            const isExpanded = state[options.expandedKey] === it.key;
            return `<button type="button" class="quality-feed__row ${isExpanded ? "is-expanded" : ""}" data-risk-key="${escapeHtml(it.key)}" aria-expanded="${isExpanded ? "true" : "false"}">
                ${flags.ranked ? `<span class="rank-pill">${index + 1}</span>` : `<span class="pill">${escapeHtml(it.kind)}</span>`}
                <span class="quality-feed__name"><code>${escapeHtml(tail)}</code><span class="muted quality-feed__detail">${escapeHtml(it.details)}</span></span>
                <span class="${cls}">${formatNumber.format(it.score)}</span>
            </button>
            ${isExpanded ? renderQualityFeedDetail(it) : ""}`;
        }).join("")}</div>`;
    }

    function riskBuckets(items, warn, bad) {
        let good = 0, medium = 0, high = 0;
        items.forEach((item) => {
            if (item.score >= bad) high++;
            else if (item.score >= warn) medium++;
            else good++;
        });
        return [
            { cls: "good", label: `< ${formatNumber.format(warn)}`, value: good },
            { cls: "warn", label: `${formatNumber.format(warn)}-${formatNumber.format(bad)}`, value: medium },
            { cls: "bad", label: `>= ${formatNumber.format(bad)}`, value: high },
        ];
    }

    function riskCategory(score, warn, bad) {
        if (score >= bad) return { cls: "bad", label: "High risk" };
        if (score >= warn) return { cls: "warn", label: "Watch" };
        return { cls: "good", label: "Healthy" };
    }

    function riskRangeCategory(low, high, warn, bad) {
        if (low >= bad) return { cls: "bad", label: "High risk" };
        if (low >= warn && high < bad) return { cls: "warn", label: "Watch" };
        if (high < warn) return { cls: "good", label: "Healthy" };
        return { cls: "mixed", label: "Mixed risk" };
    }

    function renderRiskDistributionBinPanel(bin, index, options, maxScore, bucketCount) {
        const ordered = [...bin].sort((left, right) => right.score - left.score);
        const low = (index / bucketCount) * maxScore;
        const high = ((index + 1) / bucketCount) * maxScore;
        const bucketCategory = riskRangeCategory(low, high, options.warn, options.bad);
        const rows = ordered.slice(0, 14).map((item) => {
            const title = shortenLabel(String(item.name || "Item"));
            const detail = [item.kind, item.details].filter(Boolean).join(" - ");
            const cls = riskClass(item.score, options.warn, options.bad);
            const category = riskCategory(item.score, options.warn, options.bad);
            return `<div class="performance-bin-row">
                <span class="performance-bin-row__main">
                    <strong title="${escapeHtml(item.name || title)}">${escapeHtml(title)}</strong>
                    <em>${escapeHtml(detail)}</em>
                </span>
                <span class="performance-bin-row__value ${cls}">${formatNumber.format(item.score)}</span>
                <i class="performance-bin-row__category performance-bin-row__category--${category.cls}" title="${escapeHtml(category.label)}"></i>
            </div>`;
        }).join("");
        const overflow = ordered.length > 14
            ? `<p>${formatNumber.format(ordered.length - 14)} more in this bucket. Use the dataset tables for the full list.</p>`
            : "";
        const filterField = options.filterField || "score";
        const datasetButton = options.dataset
            ? `<button type="button" class="performance-bin-popover__action" data-quality-bin-dataset="${escapeHtml(options.dataset)}" data-quality-bin-filter="${escapeHtml(`${filterField}:>=${low.toFixed(2)} ${filterField}:<${high.toFixed(2)}`)}">Show all in dataset</button>`
            : "";
        return `<div data-performance-bin-panel="${index}">
            <div class="performance-bin-popover__header">
                <strong><i class="performance-bin-row__category performance-bin-row__category--${bucketCategory.cls}"></i>${formatNumber.format(bin.length)} ${bin.length === 1 ? "item" : "items"} · ${escapeHtml(bucketCategory.label)}</strong>
                <span>${escapeHtml(formatNumber.format(low))}-${escapeHtml(formatNumber.format(high))} ${escapeHtml(options.scoreLabel || "score")}</span>
            </div>
            <div class="performance-bin-popover__list">${rows || `<p>No items in this bucket.</p>`}${overflow}${datasetButton}</div>
        </div>`;
    }

    function riskDistributionCurve(items, options) {
        const scores = items.map((item) => Number(item.score || 0)).filter(Number.isFinite);
        const total = scores.length;
        const maxScore = Math.max(options.bad * 1.2, ...scores, 1);
        const mean = scores.reduce((sum, score) => sum + score, 0) / total;
        const sortedScores = [...scores].sort((a, b) => a - b);
        const p90 = percentileValue(sortedScores, 0.9) || 0;
        const width = 640;
        const height = 190;
        const left = 30;
        const right = 610;
        const baseline = 148;
        const top = 24;
        const bucketCount = 22;
        const bins = Array.from({ length: bucketCount }, () => []);
        items.forEach((item) => {
            const score = Number(item.score || 0);
            if (!Number.isFinite(score)) return;
            const index = Math.min(bucketCount - 1, Math.floor((score / maxScore) * bucketCount));
            bins[index].push(item);
        });
        const maxBin = Math.max(...bins.map((bin) => bin.length), 1);
        const binPanels = bins.map((bin, index) => renderRiskDistributionBinPanel(bin, index, options, maxScore, bucketCount)).join("");
        const bars = bins.map((bin, index) => {
            const count = bin.length;
            const x = left + (index / bucketCount) * (right - left);
            const barWidth = ((right - left) / bucketCount) - 3;
            const barHeight = (count / maxBin) * 76;
            const low = (index / bucketCount) * maxScore;
            const high = ((index + 1) / bucketCount) * maxScore;
            const detail = [
                `${formatNumber.format(count)} ${count === 1 ? "item" : "items"}`,
                `${formatNumber.format(low)} to ${formatNumber.format(high)} ${options.scoreLabel || "score"}`,
            ].join(" - ");
            const leftPct = ((x + barWidth / 2) / width) * 100;
            const topPct = ((baseline - Math.max(barHeight, 10)) / height) * 100;
            return `<rect x="${x.toFixed(1)}" y="${(baseline - barHeight).toFixed(1)}" width="${barWidth.toFixed(1)}" height="${barHeight.toFixed(1)}" rx="3" class="risk-curve__bar" tabindex="0" role="button" aria-label="${escapeHtml(detail)}" data-performance-bin-index="${index}" data-performance-bin-left="${leftPct.toFixed(2)}" data-performance-bin-top="${topPct.toFixed(2)}"></rect>`;
        }).join("");
        const points = Array.from({ length: 80 }, (_, index) => {
            const score = (index / 79) * maxScore;
            const above = sortedScores.filter((value) => value >= score).length / Math.max(total, 1);
            const x = left + (score / maxScore) * (right - left);
            const y = baseline - (above * (baseline - top));
            return `${x.toFixed(1)},${y.toFixed(1)}`;
        });
        const path = `M ${points.join(" L ")}`;
        const marker = (score, cls) => {
            const x = left + (Math.min(score, maxScore) / maxScore) * (right - left);
            return `<line x1="${x.toFixed(1)}" x2="${x.toFixed(1)}" y1="${top}" y2="${baseline}" class="risk-curve__marker risk-curve__marker--${cls}"></line>`;
        };
        const meanX = left + (mean / maxScore) * (right - left);
        return `<div class="risk-curve-card performance-dist-curve">
            <svg class="risk-curve" viewBox="0 0 ${width} ${height}" role="img" aria-label="Empirical tail distribution for ${escapeHtml(options.scoreLabel)}">
                <line x1="${left}" x2="${right}" y1="${baseline}" y2="${baseline}" class="risk-curve__axis"></line>
                ${bars}
                ${marker(options.warn, "warn")}
                ${marker(options.bad, "bad")}
                <path d="${path}" class="risk-curve__line"></path>
                <circle cx="${meanX.toFixed(1)}" cy="${baseline - 6}" r="4" class="risk-curve__mean"></circle>
            </svg>
            <div class="performance-bin-popover" hidden></div>
            <div class="performance-bin-panels" hidden>${binPanels}</div>
            <div class="risk-curve-card__stats">
                <span><strong>${formatNumber.format(total)}</strong> items</span>
                <span><strong>${formatNumber.format(mean)}</strong> mean</span>
                <span><strong>${formatNumber.format(p90)}</strong> p90</span>
            </div>
        </div>`;
    }

    function renderQualityFeed() {
        const target = byId("quality-feed");
        if (!target) return;
        const filter = (byId("quality-feed-filter")?.value || "").toLowerCase();
        const merged = [...qualityDistributionItems(), ...cloneDistributionItems()]
            .filter((it) => !filter || it.searchText.toLowerCase().includes(filter))
            .sort((a, b) => b.score - a.score)
            .slice(0, 60);
        if (!merged.length) {
            target.innerHTML = `<p class="muted">No items match.</p>`;
            return;
        }
        target.innerHTML = merged.map((it) => {
            const cls = riskClass(it.score, 300, 600);
            const tail = (it.name || "").split(/[\\/]/).pop();
            const isExpanded = state.expandedQualityKey === it.key;
            return `<button type="button" class="quality-feed__row ${isExpanded ? "is-expanded" : ""}" data-quality-key="${escapeHtml(it.key)}" aria-expanded="${isExpanded ? "true" : "false"}">
                <span class="pill">${escapeHtml(it.kind)}</span>
                <span class="quality-feed__name"><code>${escapeHtml(tail)}</code><span class="muted quality-feed__detail">${escapeHtml(it.details)}</span></span>
                <span class="${cls}">${formatNumber.format(it.score)}</span>
            </button>
            ${isExpanded ? renderQualityFeedDetail(it) : ""}`;
        }).join("");
        target.querySelectorAll("[data-quality-key]").forEach((button) => {
            button.addEventListener("click", () => {
                state.expandedQualityKey = state.expandedQualityKey === button.dataset.qualityKey ? null : button.dataset.qualityKey;
                renderQualityFeed();
            });
        });
    }

    function renderMetricGrid(metrics) {
        return `<div class="quality-detail__metrics">${metrics.map(({ label, value, cls }) => `
            <div class="quality-detail__metric">
                <span>${escapeHtml(label)}</span>
                <strong class="${cls || ""}">${value}</strong>
            </div>`).join("")}</div>`;
    }

    function cloneTouchesFile(clone, fileName) {
        const needle = normalizePath(fileName);
        return (clone.instances || []).some((inst) => normalizePath(inst.file_path) === needle);
    }

    function slocForFile(fileName) {
        const needle = normalizePath(fileName);
        const hotspot = (state.hotspots || []).find((item) => item.kind === "unit" && normalizePath(item.name) === needle);
        return Number(hotspot?.sloc || 0);
    }

    function cloneDensity(clone) {
        const files = new Set((clone.instances || []).map((inst) => normalizePath(inst.file_path)));
        const sloc = [...files].reduce((sum, file) => sum + slocForFile(file), 0);
        return sloc > 0 ? (Number(clone.max_line_span || 0) * (clone.instances || []).length) / sloc : 0;
    }

    function hotspotTouchesClone(hotspot, clone) {
        const files = new Set((clone.instances || []).map((inst) => normalizePath(inst.file_path)));
        return files.has(normalizePath(hotspot.name));
    }

    function renderQualityFeedDetail(item) {
        if (String(item.key || "").startsWith("type:")) {
            return `<div class="quality-detail">${renderTypeHealthDetail(item.raw || {})}</div>`;
        }

        if (item.kind === "clone") {
            const clone = item.raw || {};
            const relatedHotspots = (state.hotspots || [])
                .filter((hotspot) => hotspotTouchesClone(hotspot, clone))
                .sort((a, b) => qualityScore(b) - qualityScore(a))
                .slice(0, 8);
            return `<div class="quality-detail">
                ${renderCloneDetail(clone)}
                <div class="quality-detail__section">
                    <h4>Related quality metrics</h4>
                    ${relatedHotspots.length ? relatedHotspots.map((hotspot) => renderHotspotDetail(hotspot, { compact: true })).join("") : `<p class="muted">No matching quality hotspot records.</p>`}
                </div>
            </div>`;
        }

        const hotspot = item.raw || {};
        const matchingClones = (state.clones || [])
            .filter((clone) => cloneTouchesFile(clone, hotspot.name))
            .sort((a, b) => Number(b.score || 0) - Number(a.score || 0))
            .slice(0, 8);
        return `<div class="quality-detail">
            ${renderHotspotDetail(hotspot)}
            <div class="quality-detail__section">
                <h4>Clone metrics for this file</h4>
                ${matchingClones.length ? matchingClones.map((clone) => renderCloneDetail(clone, { compact: true })).join("") : `<p class="muted">No clone groups found for this file.</p>`}
            </div>
        </div>`;
    }

    function renderTypeHealthDetail(item, options = {}) {
        const risk = typeHealthRisk(item);
        const width = Math.max(Number(item.field_count || 0), Number(item.variant_count || 0));
        const metrics = [
            { label: "Risk", value: formatNumber.format(risk), cls: riskClass(risk, 40, 70) },
            { label: "Kind", value: escapeHtml(item.kind || "type") },
            { label: "Width", value: formatNumber.format(width) },
            { label: "Fields", value: formatNumber.format(item.field_count || 0) },
            { label: "Variants", value: formatNumber.format(item.variant_count || 0) },
            { label: "Methods", value: formatNumber.format(item.method_count || 0) },
            { label: "Impl Blocks", value: formatNumber.format(item.impl_block_count || 0) },
            { label: "Impl Files", value: formatNumber.format(item.impl_file_count || 0) },
            { label: "Declaration", value: `${formatNumber.format(item.declaration_span || 0)} lines` },
        ];
        const implFiles = (item.impl_files || []).slice(0, options.compact ? 4 : 12);
        return `<div class="quality-detail__section ${options.compact ? "quality-detail__section--compact" : ""}">
            <h4>${escapeHtml(item.qualified_name || item.type_name || "Type health")}</h4>
            ${renderMetricGrid(metrics)}
            <div class="quality-detail__signals">${renderPills(item.signals)}</div>
            ${implFiles.length ? `<div class="quality-detail__locations">${implFiles.map((path) => `<code>${escapeHtml(path)}</code>`).join("")}</div>` : `<p class="muted">No impl files recorded.</p>`}
        </div>`;
    }

    function renderHotspotDetail(hotspot, options = {}) {
        const score = qualityScore(hotspot);
        const metrics = [
            { label: "Quality", value: formatNumber.format(score), cls: riskClass(score, 300, 600) },
            { label: "Cog", value: formatNumber.format(hotspot.cognitive || 0) },
            { label: "Cyc", value: formatNumber.format(hotspot.cyclomatic || 0) },
            { label: "MI", value: formatNumber.format(hotspot.mi || 0), cls: inverseRiskClass(Number(hotspot.mi || 0), 60, 40) },
            { label: "Halstead Effort", value: formatNumber.format(hotspot.effort || 0) },
            { label: "Density", value: formatNumber.format(hotspot.complexity_density || 0) },
            { label: "Bugs", value: formatNumber.format(hotspot.bugs || 0) },
            { label: "SLOC", value: formatNumber.format(hotspot.sloc || 0) },
            { label: "Start Line", value: escapeHtml(hotspot.start_line || "-") },
        ];
        return `<div class="quality-detail__section ${options.compact ? "quality-detail__section--compact" : ""}">
            <h4>${escapeHtml(hotspot.name || "Quality metrics")}</h4>
            ${renderQualityComponentBar(hotspot)}
            ${renderMetricGrid(metrics)}
            <div class="quality-detail__signals">${renderPills(hotspot.signals)}</div>
        </div>`;
    }

    function renderCloneDetail(clone, options = {}) {
        const hash = clone.hash || clone.group_hash || "";
        const instances = clone.instances || [];
        const fileCount = clone.file_count ?? new Set(instances.map((inst) => normalizePath(inst.file_path))).size;
        const score = Number(clone.score || 0);
        const metrics = [
            { label: "Engine", value: escapeHtml(clone.engine || "token") },
            { label: "Group Hash", value: `<code>${escapeHtml(hash.substring(0, options.compact ? 8 : 16) || "-")}</code>` },
            { label: "Instances", value: formatNumber.format(clone.instance_count || instances.length || 0) },
            { label: "Files", value: formatNumber.format(fileCount) },
            { label: "Score", value: formatNumber.format(score), cls: riskClass(score, 20, 40) },
            { label: "Density", value: formatNumber.format(cloneDensity(clone)) },
            { label: "Token Count", value: formatNumber.format(clone.token_count || 0) },
            { label: "Max Line Span", value: formatNumber.format(clone.max_line_span || 0) },
        ];
        const locations = instances.length
            ? `<div class="quality-detail__locations">${instances.slice(0, options.compact ? 4 : 12).map((inst) => `
                <code>${escapeHtml(inst.file_path)}:${escapeHtml(inst.start_line)}-${escapeHtml(inst.end_line)}</code>`).join("")}</div>`
            : `<p class="muted">No clone locations recorded.</p>`;
        const snippets = instances
            .filter((inst) => inst.snippet)
            .slice(0, options.compact ? 2 : 4);
        return `<div class="quality-detail__section ${options.compact ? "quality-detail__section--compact" : ""}">
            <h4>${escapeHtml(options.compact ? `Clone ${hash.substring(0, 8)}` : "Clone metrics")}</h4>
            ${renderMetricGrid(metrics)}
            <div class="quality-detail__signals">${renderPills(clone.signals)}</div>
            ${locations}
            ${snippets.length ? `<div class="clone-snippet-grid">${snippets.map((inst) => `<figure class="clone-snippet">
                <figcaption>${escapeHtml(inst.file_path)}:${escapeHtml(inst.start_line)}-${escapeHtml(inst.end_line)}</figcaption>
                <pre>${escapeHtml(inst.snippet)}</pre>
            </figure>`).join("")}</div>` : ""}
        </div>`;
    }

    function titleCaseMetricName(value) {
        return String(value || "")
            .replace(/^search_/, "")
            .replaceAll("_", " ")
            .replace(/\b\w/g, (letter) => letter.toUpperCase());
    }

    function renderQualityDatasetView() {
        document.querySelectorAll("[data-quality-dataset-view]").forEach((button) => {
            const active = button.dataset.qualityDatasetView === state.qualityDatasetView;
            button.classList.toggle("is-active", active);
            button.setAttribute("aria-pressed", active ? "true" : "false");
        });
        document.querySelectorAll("[data-quality-dataset-panel]").forEach((panel) => {
            panel.classList.toggle("is-active", panel.dataset.qualityDatasetPanel === state.qualityDatasetView);
        });
        updateQualityHash();
    }

    function qualityFilterInputForDataset(dataset) {
        return {
            hotspots: byId("hotspots-filter"),
            clones: byId("clones-filter"),
            typeHealth: byId("type-health-filter"),
            escapeHatches: byId("escape-hatches-filter"),
            locality: byId("locality-filter"),
            leverage: byId("leverage-filter"),
        }[dataset] || null;
    }

    function renderQualityDataset(dataset) {
        if (dataset === "hotspots") renderHotspots();
        else if (dataset === "clones") renderClones();
        else if (dataset === "typeHealth") renderTypeHealth();
        else if (dataset === "escapeHatches") renderEscapeHatches();
        else if (dataset === "locality" || dataset === "leverage") renderLocalityLeverage();
    }

    function drillToQualityDataset(dataset, filter = "") {
        if (!dataset) return;
        state.qualityDatasetView = dataset;
        const input = qualityFilterInputForDataset(dataset);
        if (input && filter) {
            input.value = filter;
        }
        renderQualityDataset(dataset);
        renderQualityDatasetView();
        byId("quality-datasets")?.scrollIntoView({ behavior: "smooth", block: "start" });
    }

    function clearQualityDatasetFilter(dataset) {
        const input = qualityFilterInputForDataset(dataset);
        if (input) {
            input.value = "";
        }
        renderQualityDataset(dataset);
        renderQualityDatasetView();
    }

    function renderPerformanceDatasetView() {
        renderPerformancePromiseDetail(selectedPerformancePromise());
    }

    function rerenderPerformanceEvidence() {
        renderPerformanceOverview();
        renderPerformanceHeadlineCharts();
        renderPerformanceCuratedLists();
        renderPerformanceMeasurementGaps();
        renderPerformanceDatasetView();
    }

    function selectPerformanceScenarioTab(scenarioId, { scroll = false } = {}) {
        const scenarios = state.performanceReview?.scenarios || [];
        const scenario = scenarios.find((item) => item.id === scenarioId);
        if (!scenario) return;
        state.selectedPerformanceScenarioId = scenario.id;
        renderPerformancePromiseBoard(scenarios);
        rerenderPerformanceEvidence();
        if (scroll) {
            byId("performance-promise-detail")?.scrollIntoView({ behavior: "smooth", block: "start" });
        }
    }

    function renderPerformanceFilterOptions() {
        const search = byId("performance-dataset-search");
        if (search && search.value !== state.performanceDatasetSearch) {
            search.value = state.performanceDatasetSearch;
        }
    }

    function updateFilterOptions(id, values, selected, allLabel) {
        const target = byId(id);
        if (!target) return;
        const unique = [...new Set(values.filter(Boolean).map(String))].sort();
        const previous = target.value || selected || "all";
        target.innerHTML = [
            `<option value="all">${escapeHtml(allLabel)}</option>`,
            ...unique.map((value) => `<option value="${escapeHtml(value)}">${escapeHtml(titleCaseMetricName(value))}</option>`),
        ].join("");
        target.value = unique.includes(previous) ? previous : "all";
    }

    function renderCorrectnessMatrix() {
        const target = byId("correctness-matrix");
        if (!target) return;
        const layers = state.correctness?.layers || [];
        if (!layers.length) {
            target.innerHTML = `<p class="muted">No correctness data.</p>`;
            return;
        }
        target.innerHTML = layers.map((layer) => {
            const total = layer.total || 0;
            const passed = layer.passed || 0;
            const failed = layer.failed || 0;
            const skipped = layer.skipped || 0;
            const unknown = layer.unknown || 0;
            const ratio = layer.failed_ratio != null
                ? Number(layer.failed_ratio)
                : total ? failed / total : 0;
            let cls = "ok";
            if (failed > 0) cls = "bad";
            else if (unknown > 0 && total) cls = "warn";
            else if (!total) cls = "stale";
            const pct = (n) => total ? `${(n / total) * 100}%` : "0%";
            const isActive = state.selectedLayer === layer.name;
            return `<button type="button" class="layer-matrix__cell layer-matrix__cell--${cls} ${isActive ? "is-active" : ""}" data-layer="${escapeHtml(layer.name)}">
                <div class="layer-matrix__topline">
                    <div>
                        <div class="layer-matrix__eyebrow">${formatNumber.format(total)} tests</div>
                        <div class="layer-matrix__name" title="${escapeHtml(layer.name)}">${escapeHtml(layer.name)}</div>
                    </div>
                    <span class="layer-matrix__ratio">${formatNumber.format(ratio * 100)}%</span>
                </div>
                <div class="layer-matrix__counts">
                    <span><strong class="risk-good">${passed}</strong><em>pass</em></span>
                    <span><strong class="${failed ? "risk-bad" : "muted"}">${failed}</strong><em>fail</em></span>
                    <span><strong class="muted">${skipped}</strong><em>skip</em></span>
                    <span><strong class="muted">${unknown}</strong><em>unknown</em></span>
                </div>
                <div class="layer-matrix__bar">
                    <span class="passed" style="width:${pct(passed)}"></span>
                    <span class="failed" style="width:${pct(failed)}"></span>
                    <span class="skipped" style="width:${pct(skipped)}"></span>
                    <span class="unknown" style="width:${pct(unknown)}"></span>
                </div>
                <div class="layer-matrix__footer">${cls === "ok" ? "Clear" : cls === "bad" ? "Failing" : cls === "warn" ? "Review" : "No data"} layer</div>
            </button>`;
        }).join("");
        target.querySelectorAll(".layer-matrix__cell").forEach((cell) => {
            cell.addEventListener("click", () => {
                const layer = cell.dataset.layer;
                state.selectedLayer = state.selectedLayer === layer ? null : layer;
                renderCorrectnessMatrix();
                renderCorrectness();
            });
        });
    }

    byId("viewer-version").textContent = viewerVersion;
    setupTabs();
    byId("hotspots-filter")?.addEventListener("input", () => { renderHotspots(); updateQualityHash(); });
    byId("clones-filter")?.addEventListener("input", () => { renderClones(); updateQualityHash(); });
    byId("type-health-filter")?.addEventListener("input", () => { renderTypeHealth(); updateQualityHash(); });
    byId("escape-hatches-filter")?.addEventListener("input", () => { renderEscapeHatches(); updateQualityHash(); });
    byId("correctness-filter")?.addEventListener("input", renderCorrectness);
    byId("locality-filter")?.addEventListener("input", () => { renderLocalityLeverage(); updateQualityHash(); });
    byId("leverage-filter")?.addEventListener("input", () => { renderLocalityLeverage(); updateQualityHash(); });
    byId("correctness-show-all")?.addEventListener("change", renderCorrectness);
    document.querySelectorAll("[data-quality-distribution-mode]").forEach((button) => {
        button.addEventListener("click", () => {
            state.qualityDistributionMode = button.dataset.qualityDistributionMode || "counts";
            document.querySelectorAll("[data-quality-distribution-mode]").forEach((item) => {
                const active = item === button;
                item.classList.toggle("is-active", active);
                item.setAttribute("aria-pressed", active ? "true" : "false");
            });
            renderQualityDistribution();
        });
    });
    document.querySelectorAll("[data-clone-distribution-mode]").forEach((button) => {
        button.addEventListener("click", () => {
            state.cloneDistributionMode = button.dataset.cloneDistributionMode || "counts";
            document.querySelectorAll("[data-clone-distribution-mode]").forEach((item) => {
                const active = item === button;
                item.classList.toggle("is-active", active);
                item.setAttribute("aria-pressed", active ? "true" : "false");
            });
            renderCloneDistribution();
        });
    });
    document.querySelectorAll("[data-quality-dataset-view]").forEach((button) => {
        button.addEventListener("click", () => {
            state.qualityDatasetView = button.dataset.qualityDatasetView || "hotspots";
            renderQualityDatasetView();
        });
    });
    document.querySelectorAll("[data-clear-quality-filter]").forEach((button) => {
        button.addEventListener("click", () => {
            clearQualityDatasetFilter(button.dataset.clearQualityFilter || state.qualityDatasetView);
        });
    });
    document.querySelectorAll("[data-quality-panel-mode]").forEach((button) => {
        button.addEventListener("click", () => {
            const key = button.dataset.qualityPanelMode;
            if (!key) return;
            state[key] = button.dataset.qualityPanelValue || "counts";
            document.querySelectorAll(`[data-quality-panel-mode="${CSS.escape(key)}"]`).forEach((item) => {
                const active = item === button;
                item.classList.toggle("is-active", active);
                item.setAttribute("aria-pressed", active ? "true" : "false");
            });
            renderTypeHealthDistribution();
            renderEscapeHatchDistribution();
            renderLocalityLeverage();
        });
    });
    document.querySelectorAll("[data-type-health-y]").forEach((button) => {
        button.addEventListener("click", () => {
            state.typeHealthScatterY = button.dataset.typeHealthY || "methods";
            document.querySelectorAll("[data-type-health-y]").forEach((item) => {
                const active = item === button;
                item.classList.toggle("is-active", active);
                item.setAttribute("aria-pressed", active ? "true" : "false");
            });
            renderTypeHealthScatter();
        });
    });
    renderQualityDatasetView();
    byId("performance-dataset-search")?.addEventListener("input", (event) => {
        state.performanceDatasetSearch = event.target.value || "";
        rerenderPerformanceEvidence();
    });
    byId("performance-review")?.addEventListener("input", (event) => {
        const input = event.target.closest("[data-performance-section-filter]");
        if (!input) return;
        const scenarioId = input.dataset.performanceScenarioId;
        const filterKey = input.dataset.performanceSectionFilter;
        if (!scenarioId || !filterKey) return;
        state.selectedPerformanceScenarioId = scenarioId;
        performanceBucketFilters(scenarioId)[filterKey] = input.value || "";
        renderPerformancePromiseDetail(selectedPerformancePromise());
        const replacement = document.querySelector(`[data-performance-scenario-id="${CSS.escape(scenarioId)}"][data-performance-section-filter="${CSS.escape(filterKey)}"]`);
        if (replacement) {
            replacement.focus();
            replacement.setSelectionRange(replacement.value.length, replacement.value.length);
        }
    });
    byId("performance-review")?.addEventListener("click", (event) => {
        const flamegraphItem = event.target.closest("[data-flamegraph-id]");
        if (flamegraphItem?.dataset.flamegraphId) {
            const scenario = selectedPerformancePromise();
            if (!scenario) return;
            state.selectedFlamegraphsByScenario[scenario.id] = flamegraphItem.dataset.flamegraphId;
            renderPerformancePromiseDetail(scenario);
            return;
        }
        const promiseTab = event.target.closest("[data-promise-tab]");
        if (promiseTab?.dataset.promiseTab) {
            selectPerformanceScenarioTab(promiseTab.dataset.promiseTab);
            return;
        }
        const promiseFocus = event.target.closest("[data-promise-focus]");
        if (promiseFocus?.dataset.promiseFocus) {
            selectPerformanceScenarioTab(promiseFocus.dataset.promiseFocus, { scroll: true });
            return;
        }
        const distributionMode = event.target.closest("[data-performance-distribution-mode]");
        if (distributionMode?.dataset.performanceDistributionMode && distributionMode?.dataset.performanceDistribution) {
            state.performanceDistributionModes[distributionMode.dataset.performanceDistribution] = distributionMode.dataset.performanceDistributionMode;
            renderPerformanceHeadlineCharts();
            return;
        }
        const jump = event.target.closest("[data-jump-target]");
        if (jump?.dataset.jumpTarget) {
            byId(jump.dataset.jumpTarget)?.scrollIntoView({ behavior: "smooth", block: "start" });
        }
    });
    renderPerformanceDatasetView();
    byId("app-package-refresh")?.addEventListener("click", refreshAppPackage);
    byId("app-package-clear-buffers")?.addEventListener("click", clearAppPackageBuffers);
    document.querySelectorAll("[data-app-package-view]").forEach((button) => {
        button.addEventListener("click", () => {
            state.appPackageView = button.dataset.appPackageView || "diagnostics";
            renderAppPackageDataView();
        });
    });
    byId("app-package-buffer-filter")?.addEventListener("input", renderAppPackage);
    byId("app-package-diagnostics-filter")?.addEventListener("input", renderAppPackage);
    byId("map-filter").addEventListener("input", renderMap);
    byId("map-layout").addEventListener("change", (event) => {
        state.mapLayout = event.target.value;
        renderMap();
    });
    byId("map-metric").addEventListener("change", (event) => {
        state.mapMetric = event.target.value;
        renderMap();
    });
    byId("map-focus").addEventListener("change", (event) => {
        state.focusMode = event.target.checked;
        renderMap();
    });
    byId("map-zoom").addEventListener("input", (event) => {
        state.mapZoom = Number(event.target.value);
        byId("map-zoom-value").textContent = `${Math.round(state.mapZoom * 100)}%`;
        renderMap();
    });
    document.querySelectorAll("[data-overview-risk-mode]").forEach((button) => {
        button.addEventListener("click", () => {
            state.overviewRiskMode = button.dataset.overviewRiskMode;
            document.querySelectorAll("[data-overview-risk-mode]").forEach((item) => {
                const active = item === button;
                item.classList.toggle("is-active", active);
                item.setAttribute("aria-pressed", active ? "true" : "false");
            });
            renderRiskTreemap();
        });
    });
    byId("overview-risk-filter")?.addEventListener("change", (event) => {
        state.overviewRiskFilter = event.target.value;
        renderRiskTreemap();
    });
    document.querySelectorAll("[data-run]").forEach((button) => {
        button.addEventListener("click", () => triggerRun("/api/run/all", button));
    });
    document.querySelectorAll("[data-run-category]").forEach((button) => {
        button.addEventListener("click", () => triggerRun(`/api/run/category/${encodeURIComponent(button.dataset.runCategory)}`, button));
    });
    document.querySelectorAll("[data-run-item]").forEach((button) => {
        button.addEventListener("click", () => triggerRun(`/api/run/item/${encodeURIComponent(button.dataset.runItem)}`, button));
    });
    window.setInterval(refreshRuns, 5000);
    applyQualityHashState();
    loadDefaults().then(() => {
        activateTab(initialTabFromLocation());
        renderQualityDatasetView();
    });
})();
