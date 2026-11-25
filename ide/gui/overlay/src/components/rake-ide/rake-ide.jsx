// The Rake IDE panel: project management, a Monaco editor connected to rake-lsp,
// live compilation into the Scratch VM, and Rake Scene management.

import PropTypes from 'prop-types';
import React from 'react';
import {connect} from 'react-redux';
import VM from 'scratch-vm';

import api from '../../lib/rake/api.js';
import {makeTranslator} from '../../lib/rake/i18n.js';
import {RakeLanguageClient} from '../../lib/rake/lsp-client.js';
import {languageForPath, loadMonaco} from '../../lib/rake/monaco.js';
import {exportStageToScene, newSceneSource, safeIdentifier} from '../../lib/rake/scene-export.js';
import {getIsShowingProject} from '../../reducers/project-state.js';
import {setProjectTitle} from '../../reducers/project-title.js';

import styles from './rake-ide.css';

const TEXT_EXTENSIONS = /\.(rk|rks|json|md|txt|svg|csv)$/i;
const IMAGE_EXTENSIONS = /\.(svg|png|jpe?g|gif|bmp)$/i;
const AUDIO_EXTENSIONS = /\.(wav|mp3)$/i;
const STORAGE_KEY = 'rake-ide';

const loadSettings = () => {
    try {
        return JSON.parse(localStorage.getItem(STORAGE_KEY)) || {};
    } catch (e) {
        return {};
    }
};

const saveSettings = settings => {
    try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify({...loadSettings(), ...settings}));
    } catch (e) {
        // Storage can be unavailable (private mode); settings then last for this session only.
    }
};

const basename = filePath => filePath.slice(filePath.lastIndexOf('/') + 1);
const dirname = filePath => (filePath.includes('/') ? filePath.slice(0, filePath.lastIndexOf('/')) : '');

class RakeIDE extends React.Component {
    constructor (props) {
        super(props);
        const settings = loadSettings();
        this.state = {
            serverError: null,
            projects: [],
            project: null,
            openTabs: [],
            activePath: null,
            dirty: [],
            collapsed: settings.collapsedFolders || [],
            sidebarTab: settings.sidebarTab || 'files',
            activeScene: null,
            build: {status: 'idle'},
            autoBuild: settings.autoBuild !== false,
            delay: settings.delay || 800,
            keepRunning: settings.keepRunning !== false,
            diagnostics: {},
            buildError: null,
            bottomTab: 'problems',
            log: [],
            lspStatus: 'connecting',
            width: settings.width || 520,
            hidden: false,
            monacoReady: false
        };
        this.models = new Map();
        this.viewStates = new Map();
        this.saveTimer = null;
        this.building = false;
        this.buildQueued = false;
        this.firstBuildDone = false;
        this.editorHost = React.createRef();
        this.uploadInput = React.createRef();
    }

    componentDidMount () {
        this.start();
        window.addEventListener('beforeunload', this.handleBeforeUnload);
    }

    componentDidUpdate (prevProps) {
        if (prevProps.isDark !== this.props.isDark && this.monaco) {
            this.monaco.editor.setTheme(this.props.isDark ? 'rake-dark' : 'rake-light');
        }
        if (!prevProps.isShowingProject && this.props.isShowingProject && !this.firstBuildDone) {
            this.requestBuild(0);
        }
    }

    componentWillUnmount () {
        window.removeEventListener('beforeunload', this.handleBeforeUnload);
        if (this.lsp) this.lsp.dispose();
        if (this.editor) this.editor.dispose();
        this.models.forEach(model => model.dispose());
    }

    handleBeforeUnload = event => {
        if (this.state.dirty.length) {
            this.saveAll();
            event.preventDefault();
            event.returnValue = '';
        }
    };

    t = (key, values) => makeTranslator(this.props.locale)(key, values);

    log (message, level = 'info') {
        const time = new Date().toLocaleTimeString();
        this.setState(state => ({log: [...state.log.slice(-199), {time, message, level}]}));
    }

    async start () {
        try {
            await api.info();
        } catch (e) {
            this.setState({serverError: this.t('serverMissing')});
            return;
        }
        try {
            const monaco = await loadMonaco();
            this.monaco = monaco;
            this.createEditor();
            this.lsp = new RakeLanguageClient(monaco, {
                onStatus: lspStatus => this.setState({lspStatus}),
                openUri: (uri, focus) => this.openUri(uri, focus)
            });
            this.lsp.onDiagnostics((uri, diagnostics) => {
                this.setState(state => ({diagnostics: {...state.diagnostics, [uri]: diagnostics}}));
            });
            this.lsp.start().catch(error => this.log(error.message, 'error'));
            this.setState({monacoReady: true});
        } catch (error) {
            this.log(`Monaco: ${error.message}`, 'error');
        }
        await this.refreshProjects();
        const settings = loadSettings();
        const names = this.state.projects.map(project => project.name);
        const initial = names.includes(settings.project) ? settings.project : names[0];
        if (initial) await this.openProject(initial);
    }

    createEditor () {
        const monaco = this.monaco;
        this.editor = monaco.editor.create(this.editorHost.current, {
            automaticLayout: true,
            theme: this.props.isDark ? 'rake-dark' : 'rake-light',
            fontSize: 13,
            minimap: {enabled: false},
            tabSize: 4,
            insertSpaces: true,
            scrollBeyondLastLine: false,
            'semanticHighlighting.enabled': true,
            fixedOverflowWidgets: true,
            model: null
        });
        this.editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => this.saveAll());
        this.editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter, () => this.requestBuild(0));
        // Opening another file from "Go to Definition" switches the IDE tab.
        this.opener = monaco.editor.registerEditorOpener({
            openCodeEditor: (source, resource, selection) => {
                const relative = this.pathFromUri(resource.toString());
                if (!relative) return false;
                this.openFile(relative).then(() => {
                    if (!selection) return;
                    if (selection.startLineNumber) {
                        this.editor.setSelection(selection);
                        this.editor.revealRangeInCenter(selection);
                    } else {
                        this.editor.setPosition(selection);
                        this.editor.revealPositionInCenter(selection);
                    }
                });
                return true;
            }
        });
    }

    // ---- Projects ----

    async refreshProjects () {
        try {
            const projects = await api.listProjects();
            this.setState({projects, serverError: null});
        } catch (error) {
            this.setState({serverError: this.t('serverMissing')});
        }
    }

    async openProject (name) {
        await this.saveAll();
        this.closeAllModels();
        const project = await api.getProject(name);
        const scenes = project.files.filter(file => file.path.endsWith('.rks')).map(file => file.path);
        const remembered = (loadSettings().activeScenes || {})[name];
        const activeScene = scenes.includes(remembered) ? remembered :
            (scenes.includes(project.manifest.mainScene) ? project.manifest.mainScene : scenes[0] || null);
        const rememberedTabs = ((loadSettings().openTabs || {})[name] || [])
            .filter(tab => project.files.some(file => file.path === tab));
        this.setState({project, activeScene, openTabs: [], activePath: null, buildError: null, diagnostics: {}});
        saveSettings({project: name});
        this.props.onSetProjectTitle(name);
        const tabs = rememberedTabs.length ? rememberedTabs : [activeScene].filter(Boolean);
        for (const tab of tabs) await this.openFile(tab, false);
        if (tabs.length) await this.openFile(tabs[0]);
        this.log(`${this.t('project')}: ${name}`);
        if (this.props.isShowingProject) this.requestBuild(0);
    }

    async reloadProjectFiles () {
        if (!this.state.project) return;
        const project = await api.getProject(this.state.project.name);
        this.setState({project});
    }

    promptName (label, value = '') {
        // eslint-disable-next-line no-alert
        const result = window.prompt(label, value);
        return result === null ? null : result.trim() || null;
    }

    async runAction (task) {
        try {
            await task();
        } catch (error) {
            this.log(error.message, 'error');
            // eslint-disable-next-line no-alert
            window.alert(error.message);
        }
    }

    handleProjectMenu = event => {
        const action = event.target.value;
        event.target.value = '';
        const project = this.state.project;
        this.runAction(async () => {
            if (action === 'new') {
                const name = this.promptName(this.t('projectName'));
                if (!name) return;
                await api.createProject(name);
                await this.refreshProjects();
                await this.openProject(name);
            } else if (action === 'duplicate' && project) {
                const name = this.promptName(this.t('projectName'), `${project.name} copy`);
                if (!name) return;
                await this.saveAll();
                await api.duplicateProject(project.name, name);
                await this.refreshProjects();
                await this.openProject(name);
            } else if (action === 'rename' && project) {
                const name = this.promptName(this.t('projectName'), project.name);
                if (!name || name === project.name) return;
                await this.saveAll();
                this.closeAllModels();
                await api.renameProject(project.name, name);
                await this.refreshProjects();
                await this.openProject(name);
            } else if (action === 'delete' && project) {
                const info = await api.info();
                // eslint-disable-next-line no-alert
                if (!window.confirm(this.t('confirmDeleteProject', {
                    name: project.name, path: `${info.projectsDirectory}/.trash`
                }))) return;
                this.closeAllModels();
                await api.trashProject(project.name);
                this.setState({project: null, openTabs: [], activePath: null});
                await this.refreshProjects();
                if (this.state.projects.length) await this.openProject(this.state.projects[0].name);
            } else if (action === 'download' && project && this.state.activeScene) {
                await this.saveAll();
                window.open(api.downloadUrl(project.name, this.state.activeScene), '_blank');
            }
        });
    };

    handleSelectProject = event => {
        const name = event.target.value;
        this.runAction(() => this.openProject(name));
    };

    // ---- Files and editor models ----

    uriFor (relativePath) {
        return this.monaco.Uri.file(`${this.state.project.root}/${relativePath}`);
    }

    pathFromUri (uri) {
        if (!this.state.project || !this.monaco) return null;
        const target = this.monaco.Uri.parse(uri).fsPath.replace(/\\/g, '/');
        const root = this.monaco.Uri.file(this.state.project.root).fsPath.replace(/\\/g, '/');
        if (!target.toLowerCase().startsWith(`${root.toLowerCase()}/`)) return null;
        return target.slice(root.length + 1);
    }

    async ensureModel (relativePath) {
        if (this.models.has(relativePath)) return this.models.get(relativePath);
        const text = await api.readText(this.state.project.name, relativePath);
        if (this.models.has(relativePath)) return this.models.get(relativePath);
        const uri = this.uriFor(relativePath);
        const existing = this.monaco.editor.getModel(uri);
        if (existing) existing.dispose();
        const model = this.monaco.editor.createModel(text, languageForPath(relativePath), uri);
        model.onDidChangeContent(() => this.handleModelChange(relativePath, model));
        this.models.set(relativePath, model);
        if (this.lsp) this.lsp.openModel(model);
        return model;
    }

    async openUri (uri, focus = true) {
        const relative = this.pathFromUri(uri);
        if (!relative) return;
        if (focus) await this.openFile(relative);
        else await this.ensureModel(relative);
    }

    async openFile (relativePath, focus = true) {
        if (!this.state.project) return;
        const tabs = this.state.openTabs.includes(relativePath) ?
            this.state.openTabs : [...this.state.openTabs, relativePath];
        this.setState({openTabs: tabs});
        this.rememberTabs(tabs);
        if (!focus) {
            if (TEXT_EXTENSIONS.test(relativePath) && this.monaco) await this.ensureModel(relativePath);
            return;
        }
        if (this.editor && this.state.activePath && this.models.has(this.state.activePath)) {
            this.viewStates.set(this.state.activePath, this.editor.saveViewState());
        }
        this.setState({activePath: relativePath});
        if (!TEXT_EXTENSIONS.test(relativePath) || !this.editor) return;
        try {
            const model = await this.ensureModel(relativePath);
            this.editor.setModel(model);
            const viewState = this.viewStates.get(relativePath);
            if (viewState) this.editor.restoreViewState(viewState);
            this.editor.focus();
        } catch (error) {
            this.log(error.message, 'error');
        }
    }

    rememberTabs (tabs) {
        if (!this.state.project) return;
        const openTabs = loadSettings().openTabs || {};
        saveSettings({openTabs: {...openTabs, [this.state.project.name]: tabs}});
    }

    closeTab (relativePath, event) {
        if (event) event.stopPropagation();
        if (this.state.dirty.includes(relativePath)) this.saveFile(relativePath);
        const tabs = this.state.openTabs.filter(tab => tab !== relativePath);
        this.setState({openTabs: tabs});
        this.rememberTabs(tabs);
        if (this.state.activePath === relativePath) {
            const next = tabs[tabs.length - 1];
            if (next) this.openFile(next);
            else {
                this.setState({activePath: null});
                if (this.editor) this.editor.setModel(null);
            }
        }
    }

    closeAllModels () {
        if (this.editor) this.editor.setModel(null);
        this.models.forEach(model => {
            if (this.lsp) this.lsp.closeModel(model);
            model.dispose();
        });
        this.models.clear();
        this.viewStates.clear();
        this.setState({dirty: []});
    }

    handleModelChange (relativePath, model) {
        if (this.lsp) this.lsp.changeModel(model);
        if (!this.state.dirty.includes(relativePath)) {
            this.setState(state => ({dirty: [...state.dirty, relativePath]}));
        }
        clearTimeout(this.saveTimer);
        this.saveTimer = setTimeout(async () => {
            await this.saveAll();
            if (this.state.autoBuild) this.requestBuild(0);
        }, this.state.delay);
    }

    async saveFile (relativePath) {
        const model = this.models.get(relativePath);
        if (!model || !this.state.project) return;
        const text = model.getValue();
        const versionId = model.getAlternativeVersionId();
        await api.writeFile(this.state.project.name, relativePath, text);
        // Still dirty if the user typed while saving.
        if (model.getAlternativeVersionId() === versionId) {
            this.setState(state => ({dirty: state.dirty.filter(item => item !== relativePath)}));
        }
    }

    async saveAll () {
        clearTimeout(this.saveTimer);
        const dirty = [...this.state.dirty];
        for (const relativePath of dirty) {
            try {
                await this.saveFile(relativePath);
            } catch (error) {
                this.log(`${relativePath}: ${error.message}`, 'error');
            }
        }
    }

    handleNewFile = () => this.runAction(async () => {
        const folder = this.state.activePath ? dirname(this.state.activePath) : 'src';
        const relativePath = this.promptName(this.t('fileName'), `${folder ? `${folder}/` : ''}new.rk`);
        if (!relativePath) return;
        const initial = relativePath.endsWith('.rks') ? newSceneSource(basename(relativePath).replace(/\.rks$/, '')) : '';
        await api.writeFile(this.state.project.name, relativePath, initial);
        await this.reloadProjectFiles();
        await this.openFile(relativePath);
    });

    handleNewFolder = () => this.runAction(async () => {
        const relativePath = this.promptName(this.t('fileName'), 'assets/');
        if (!relativePath) return;
        await api.makeDirectory(this.state.project.name, relativePath.replace(/\/$/, ''));
        await this.reloadProjectFiles();
    });

    handleUploadClick = () => this.uploadInput.current.click();

    handleUpload = event => {
        const files = Array.from(event.target.files || []);
        event.target.value = '';
        this.runAction(async () => {
            const selected = this.state.activePath;
            const folder = selected && !selected.endsWith('.rk') && !selected.endsWith('.rks') ?
                dirname(selected) : 'assets';
            for (const file of files) {
                await api.writeFile(this.state.project.name, `${folder}/${file.name}`, file);
                this.log(`+ ${folder}/${file.name}`);
            }
            await this.reloadProjectFiles();
            if (this.state.autoBuild) this.requestBuild(0);
        });
    };

    renameFile (relativePath) {
        this.runAction(async () => {
            const target = this.promptName(this.t('fileName'), relativePath);
            if (!target || target === relativePath) return;
            await this.saveAll();
            const model = this.models.get(relativePath);
            if (model) {
                if (this.lsp) this.lsp.closeModel(model);
                if (this.editor && this.editor.getModel() === model) this.editor.setModel(null);
                model.dispose();
                this.models.delete(relativePath);
            }
            await api.moveFile(this.state.project.name, relativePath, target);
            const manifest = this.state.project.manifest;
            if (manifest.mainScene === relativePath) {
                await api.writeManifest(this.state.project.name, {...manifest, mainScene: target});
            }
            const tabs = this.state.openTabs.map(tab => (tab === relativePath ? target : tab));
            this.setState(state => ({
                openTabs: tabs,
                activeScene: state.activeScene === relativePath ? target : state.activeScene
            }));
            await this.reloadProjectFiles();
            if (this.state.activePath === relativePath) await this.openFile(target);
        });
    }

    trashFile (relativePath) {
        this.runAction(async () => {
            // eslint-disable-next-line no-alert
            if (!window.confirm(this.t('confirmDeleteFile', {name: relativePath}))) return;
            const affected = [...this.models.keys()].filter(key => key === relativePath || key.startsWith(`${relativePath}/`));
            for (const key of affected) {
                const model = this.models.get(key);
                if (this.lsp) this.lsp.closeModel(model);
                if (this.editor && this.editor.getModel() === model) this.editor.setModel(null);
                model.dispose();
                this.models.delete(key);
            }
            await api.trashFile(this.state.project.name, relativePath);
            const tabs = this.state.openTabs.filter(tab => !affected.includes(tab) && tab !== relativePath);
            this.setState(state => ({
                openTabs: tabs,
                dirty: state.dirty.filter(item => !affected.includes(item)),
                activePath: tabs.includes(state.activePath) ? state.activePath : null,
                activeScene: state.activeScene === relativePath ? null : state.activeScene
            }));
            await this.reloadProjectFiles();
        });
    }

    toggleFolder (folder) {
        const collapsed = this.state.collapsed.includes(folder) ?
            this.state.collapsed.filter(item => item !== folder) : [...this.state.collapsed, folder];
        this.setState({collapsed});
        saveSettings({collapsedFolders: collapsed});
    }

    // ---- Building and the VM ----

    requestBuild (delay) {
        clearTimeout(this.buildTimer);
        this.buildTimer = setTimeout(() => this.build(), delay);
    }

    async build () {
        const {project, activeScene} = this.state;
        if (!project || !activeScene || !this.props.isShowingProject) return;
        if (this.building) {
            this.buildQueued = true;
            return;
        }
        this.building = true;
        this.setState({build: {status: 'building'}});
        try {
            await this.saveAll();
            const result = await api.build(project.name, activeScene);
            this.setBuildMarkers(result.ok ? null : result.error);
            if (result.ok) {
                await this.loadIntoVm(result.sb3);
                this.firstBuildDone = true;
                this.setState({build: {status: 'ok', blocks: result.blocks, elapsed: result.elapsed}, buildError: null});
                this.log(`${activeScene}: ${this.t('built', {blocks: result.blocks, ms: result.elapsed})}`);
            } else {
                this.setState({build: {status: 'error'}, buildError: result.error});
                const where = result.error.file ? `${result.error.file}:${result.error.line}:${result.error.column}: ` : '';
                this.log(`${this.t('buildFailed')}: ${where}${result.error.message}`, 'error');
            }
        } catch (error) {
            this.setState({build: {status: 'error'}, buildError: {file: null, line: 1, column: 1, message: error.message}});
            this.log(error.message, 'error');
        } finally {
            this.building = false;
            if (this.buildQueued) {
                this.buildQueued = false;
                this.requestBuild(0);
            }
        }
    }

    setBuildMarkers (error) {
        if (!this.monaco) return;
        this.models.forEach(model => this.monaco.editor.setModelMarkers(model, 'rake-build', []));
        if (!error || !error.file) return;
        const model = this.models.get(error.file);
        if (!model) return;
        this.monaco.editor.setModelMarkers(model, 'rake-build', [{
            startLineNumber: error.line,
            startColumn: error.column,
            endLineNumber: error.line,
            endColumn: error.column + 1,
            message: error.message,
            severity: this.monaco.MarkerSeverity.Error,
            source: 'rake build'
        }]);
    }

    async loadIntoVm (sb3) {
        const vm = this.props.vm;
        const editing = vm.editingTarget;
        const editingName = editing ? (editing.isStage ? null : editing.getName()) : null;
        const wasRunning = this.state.keepRunning && vm.runtime.threads.some(thread => !thread.updateMonitor);
        await vm.loadProject(sb3);
        const target = editingName ? vm.runtime.getSpriteTargetByName(editingName) : null;
        if (target) vm.setEditingTarget(target.id);
        if (wasRunning) vm.greenFlag();
    }

    handleBuildClick = () => this.requestBuild(0);

    handleToggleAutoBuild = () => {
        const autoBuild = !this.state.autoBuild;
        this.setState({autoBuild});
        saveSettings({autoBuild});
        if (autoBuild) this.requestBuild(0);
    };

    handleToggleKeepRunning = () => {
        const keepRunning = !this.state.keepRunning;
        this.setState({keepRunning});
        saveSettings({keepRunning});
    };

    handleDelayChange = event => {
        const delay = Number(event.target.value);
        this.setState({delay});
        saveSettings({delay});
    };

    // ---- Scenes ----

    scenes () {
        if (!this.state.project) return [];
        return this.state.project.files.filter(file => file.path.endsWith('.rks')).map(file => file.path);
    }

    setActiveScene (scenePath) {
        this.setState({activeScene: scenePath}, () => this.requestBuild(0));
        const activeScenes = loadSettings().activeScenes || {};
        saveSettings({activeScenes: {...activeScenes, [this.state.project.name]: scenePath}});
    }

    handleSceneSelect = event => this.setActiveScene(event.target.value);

    handleNewScene = () => this.runAction(async () => {
        const name = this.promptName(this.t('sceneName'), 'Level');
        if (!name) return;
        const scenePath = `scenes/${safeIdentifier(name)}.rks`;
        await api.writeFile(this.state.project.name, scenePath, newSceneSource(name));
        await this.reloadProjectFiles();
        await this.openFile(scenePath);
        this.setActiveScene(scenePath);
    });

    duplicateScene (scenePath) {
        this.runAction(async () => {
            const name = this.promptName(this.t('sceneName'), `${basename(scenePath).replace(/\.rks$/, '')}_copy`);
            if (!name) return;
            await this.saveAll();
            const text = await api.readText(this.state.project.name, scenePath);
            const target = `${dirname(scenePath) || 'scenes'}/${safeIdentifier(name)}.rks`;
            const renamed = text.replace(/\bscene\s+[A-Za-z_]\w*/, `scene ${safeIdentifier(name)}`);
            await api.writeFile(this.state.project.name, target, renamed);
            await this.reloadProjectFiles();
            await this.openFile(target);
        });
    }

    setMainScene (scenePath) {
        this.runAction(async () => {
            const manifest = await api.writeManifest(this.state.project.name, {
                ...this.state.project.manifest, mainScene: scenePath
            });
            this.setState(state => ({project: {...state.project, manifest}}));
        });
    }

    exportScene (scenePath) {
        this.runAction(async () => {
            // eslint-disable-next-line no-alert
            if (!window.confirm(this.t('confirmExport', {path: scenePath}))) return;
            await this.saveAll();
            let previous = null;
            try {
                previous = await api.describeScene(this.state.project.name, scenePath);
            } catch (e) {
                previous = null; // An invalid scene is replaced entirely.
            }
            const model = this.models.get(scenePath);
            const sceneName = previous && previous.name ? previous.name :
                basename(scenePath).replace(/\.rks$/, '');
            const result = await exportStageToScene({
                vm: this.props.vm,
                api,
                project: this.state.project.name,
                scenePath,
                sceneName,
                previous
            });
            if (model) {
                // Replace the open buffer without marking it dirty; the file is already written.
                model.pushEditOperations([], [{range: model.getFullModelRange(), text: result.text}], () => null);
                this.setState(state => ({dirty: state.dirty.filter(item => item !== scenePath)}));
            }
            for (const file of result.written) this.log(`↳ ${file}`);
            if (result.skipped.length) this.log(`skipped sounds: ${result.skipped.join(', ')}`, 'error');
            this.log(this.t('exported', {count: result.sprites.length, path: scenePath}));
            await this.reloadProjectFiles();
        });
    }

    // ---- Problems ----

    problems () {
        const problems = [];
        for (const [uri, diagnostics] of Object.entries(this.state.diagnostics)) {
            const relativePath = this.pathFromUri(uri) || uri;
            for (const diagnostic of diagnostics) {
                problems.push({
                    file: relativePath,
                    line: diagnostic.range.start.line + 1,
                    column: diagnostic.range.start.character + 1,
                    message: diagnostic.message,
                    source: diagnostic.source || 'rake'
                });
            }
        }
        const error = this.state.buildError;
        if (error && !problems.some(problem => problem.file === error.file && problem.line === error.line &&
            problem.message === error.message)) {
            problems.push({...error, source: 'build'});
        }
        return problems;
    }

    revealProblem (problem) {
        if (!problem.file) return;
        this.openFile(problem.file).then(() => {
            if (!this.editor) return;
            const position = {lineNumber: problem.line, column: problem.column};
            this.editor.setPosition(position);
            this.editor.revealPositionInCenter(position);
            this.editor.focus();
        });
    }

    // ---- Layout ----

    handleResizeStart = event => {
        event.preventDefault();
        const startX = event.clientX;
        const startWidth = this.state.width;
        const move = moveEvent => {
            const width = Math.min(Math.max(280, startWidth - (moveEvent.clientX - startX)), window.innerWidth - 600);
            this.setState({width});
        };
        const up = () => {
            window.removeEventListener('mousemove', move);
            window.removeEventListener('mouseup', up);
            saveSettings({width: this.state.width});
            window.dispatchEvent(new Event('resize'));
        };
        window.addEventListener('mousemove', move);
        window.addEventListener('mouseup', up);
    };

    handleToggleHidden = () => {
        this.setState(state => ({hidden: !state.hidden}), () => window.dispatchEvent(new Event('resize')));
    };

    // ---- Rendering ----

    renderFileTree () {
        const {project, activePath, collapsed, dirty} = this.state;
        if (!project) return null;
        const items = project.files.filter(file => {
            const parts = file.path.split('/');
            for (let i = 1; i < parts.length; i++) {
                if (collapsed.includes(parts.slice(0, i).join('/'))) return false;
            }
            return true;
        });
        return (
            <div className={styles.tree}>
                {items.map(file => {
                    const depth = file.path.split('/').length - 1;
                    const name = basename(file.path);
                    const isOpen = !collapsed.includes(file.path);
                    return (
                        <div
                            className={`${styles.treeItem} ${activePath === file.path ? styles.selected : ''}`}
                            key={file.path}
                            style={{paddingLeft: 6 + (depth * 12)}}
                            title={file.path}
                            onClick={() => (file.directory ? this.toggleFolder(file.path) : this.openFile(file.path))}
                        >
                            <span className={styles.treeIcon}>
                                {file.directory ? (isOpen ? '▾' : '▸') : fileIcon(file.path)}
                            </span>
                            <span className={styles.treeName}>
                                {name}{dirty.includes(file.path) ? ' •' : ''}
                            </span>
                            <span className={styles.treeActions}>
                                <button
                                    title={this.t('rename')}
                                    onClick={event => {
                                        event.stopPropagation();
                                        this.renameFile(file.path);
                                    }}
                                >{'✎'}</button>
                                <button
                                    title={this.t('delete')}
                                    onClick={event => {
                                        event.stopPropagation();
                                        this.trashFile(file.path);
                                    }}
                                >{'🗑'}</button>
                            </span>
                        </div>
                    );
                })}
            </div>
        );
    }

    renderScenes () {
        const {project, activeScene} = this.state;
        if (!project) return null;
        const mainScene = project.manifest.mainScene;
        return (
            <div className={styles.tree}>
                {this.scenes().map(scenePath => (
                    <div
                        className={`${styles.sceneItem} ${activeScene === scenePath ? styles.selected : ''}`}
                        key={scenePath}
                    >
                        <div
                            className={styles.sceneTitle}
                            title={scenePath}
                            onClick={() => this.setActiveScene(scenePath)}
                        >
                            <span className={styles.treeIcon}>{activeScene === scenePath ? '●' : '○'}</span>
                            <span className={styles.treeName}>{basename(scenePath).replace(/\.rks$/, '')}</span>
                            {mainScene === scenePath ? <span className={styles.badge}>{this.t('mainScene')}</span> : null}
                        </div>
                        <div className={styles.sceneActions}>
                            <button
                                title={this.t('runScene')}
                                onClick={() => this.setActiveScene(scenePath)}
                            >{'▶'}</button>
                            <button
                                title={this.t('openScene')}
                                onClick={() => this.openFile(scenePath)}
                            >{'✎'}</button>
                            <button
                                title={this.t('exportSceneTooltip')}
                                onClick={() => this.exportScene(scenePath)}
                            >{this.t('exportScene')}</button>
                            <button
                                title={this.t('setMainScene')}
                                disabled={mainScene === scenePath}
                                onClick={() => this.setMainScene(scenePath)}
                            >{'★'}</button>
                            <button
                                title={this.t('duplicateScene')}
                                onClick={() => this.duplicateScene(scenePath)}
                            >{'⧉'}</button>
                            <button
                                title={this.t('rename')}
                                onClick={() => this.renameFile(scenePath)}
                            >{'Aa'}</button>
                            <button
                                title={this.t('delete')}
                                onClick={() => this.trashFile(scenePath)}
                            >{'🗑'}</button>
                        </div>
                    </div>
                ))}
            </div>
        );
    }

    renderPreview () {
        const {project, activePath} = this.state;
        if (!project || !activePath || TEXT_EXTENSIONS.test(activePath)) return null;
        const url = `/api/projects/${encodeURIComponent(project.name)}/files/${activePath.split('/').map(encodeURIComponent)
            .join('/')}`;
        let body;
        if (IMAGE_EXTENSIONS.test(activePath)) {
            body = (<img
                alt={activePath}
                src={url}
            />);
        } else if (AUDIO_EXTENSIONS.test(activePath)) {
            body = (<audio
                controls
                src={url}
            />);
        } else {
            body = <span>{this.t('binaryFile')}</span>;
        }
        return <div className={styles.preview}>{body}</div>;
    }

    renderStatus () {
        const {build} = this.state;
        if (build.status === 'building') return <span className={styles.statusBusy}>{this.t('building')}</span>;
        if (build.status === 'error') return <span className={styles.statusError}>{this.t('buildFailed')}</span>;
        if (build.status === 'ok') {
            return (<span className={styles.statusOk}>
                {this.t('built', {blocks: build.blocks, ms: build.elapsed})}
            </span>);
        }
        return null;
    }

    render () {
        const t = this.t;
        const {project, projects, openTabs, activePath, dirty, sidebarTab, activeScene, bottomTab, log} = this.state;
        if (this.state.hidden) {
            return (
                <div className={styles.collapsedBar}>
                    <button
                        title={t('showPanel')}
                        onClick={this.handleToggleHidden}
                    >{'‹'}</button>
                </div>
            );
        }
        const problems = this.problems();
        return (
            <div
                className={`${styles.ide} ${this.props.isDark ? styles.dark : ''}`}
                style={{width: this.state.width}}
            >
                <div
                    className={styles.resizer}
                    onMouseDown={this.handleResizeStart}
                />
                <div className={styles.toolbar}>
                    <select
                        className={styles.projectSelect}
                        title={t('project')}
                        value={project ? project.name : ''}
                        onChange={this.handleSelectProject}
                    >
                        {project ? null : <option value="">{t('project')}</option>}
                        {projects.map(item => (
                            <option
                                key={item.name}
                                value={item.name}
                            >{item.name}</option>
                        ))}
                    </select>
                    <select
                        className={styles.menuSelect}
                        title={t('projects')}
                        value=""
                        onChange={this.handleProjectMenu}
                    >
                        <option value="">{'☰'}</option>
                        <option value="new">{t('newProject')}</option>
                        <option
                            disabled={!project}
                            value="duplicate"
                        >{t('duplicateProject')}</option>
                        <option
                            disabled={!project}
                            value="rename"
                        >{t('renameProject')}</option>
                        <option
                            disabled={!project || !activeScene}
                            value="download"
                        >{t('download')}</option>
                        <option
                            disabled={!project}
                            value="delete"
                        >{t('deleteProject')}</option>
                    </select>
                    <span className={styles.label}>{t('activeScene')}</span>
                    <select
                        className={styles.sceneSelect}
                        value={activeScene || ''}
                        onChange={this.handleSceneSelect}
                    >
                        {this.scenes().map(scenePath => (
                            <option
                                key={scenePath}
                                value={scenePath}
                            >{basename(scenePath)}</option>
                        ))}
                    </select>
                    <button
                        className={styles.buildButton}
                        disabled={!project || !activeScene}
                        title={t('buildTooltip')}
                        onClick={this.handleBuildClick}
                    >{'▶ '}{t('build')}</button>
                    <label
                        className={styles.toggle}
                        title={t('autoBuildTooltip')}
                    >
                        <input
                            checked={this.state.autoBuild}
                            type="checkbox"
                            onChange={this.handleToggleAutoBuild}
                        />
                        {t('autoBuild')}
                    </label>
                    <select
                        className={styles.delaySelect}
                        title={t('delay')}
                        value={this.state.delay}
                        onChange={this.handleDelayChange}
                    >
                        {[300, 500, 800, 1200, 2000].map(ms => (
                            <option
                                key={ms}
                                value={ms}
                            >{`${ms} ms`}</option>
                        ))}
                    </select>
                    <label
                        className={styles.toggle}
                        title={t('restoreRunning')}
                    >
                        <input
                            checked={this.state.keepRunning}
                            type="checkbox"
                            onChange={this.handleToggleKeepRunning}
                        />
                        {'⚑'}
                    </label>
                    <span className={styles.spacer} />
                    {this.renderStatus()}
                    <button
                        className={styles.iconButton}
                        title={t('hidePanel')}
                        onClick={this.handleToggleHidden}
                    >{'›'}</button>
                </div>
                {this.state.serverError ? <div className={styles.banner}>{this.state.serverError}</div> : null}
                <div className={styles.body}>
                    <div className={styles.sidebar}>
                        <div className={styles.sidebarTabs}>
                            <button
                                className={sidebarTab === 'files' ? styles.activeTab : ''}
                                onClick={() => {
                                    this.setState({sidebarTab: 'files'});
                                    saveSettings({sidebarTab: 'files'});
                                }}
                            >{t('files')}</button>
                            <button
                                className={sidebarTab === 'scenes' ? styles.activeTab : ''}
                                onClick={() => {
                                    this.setState({sidebarTab: 'scenes'});
                                    saveSettings({sidebarTab: 'scenes'});
                                }}
                            >{t('scenes')}</button>
                        </div>
                        <div className={styles.sidebarActions}>
                            {sidebarTab === 'files' ? (
                                <React.Fragment>
                                    <button
                                        disabled={!project}
                                        title={t('newFile')}
                                        onClick={this.handleNewFile}
                                    >{'+ 📄'}</button>
                                    <button
                                        disabled={!project}
                                        title={t('newFolder')}
                                        onClick={this.handleNewFolder}
                                    >{'+ 📁'}</button>
                                    <button
                                        disabled={!project}
                                        title={t('upload')}
                                        onClick={this.handleUploadClick}
                                    >{'⇪'}</button>
                                    <input
                                        multiple
                                        accept=".svg,.png,.jpg,.jpeg,.gif,.bmp,.wav,.mp3,.rk,.rks"
                                        className={styles.hidden}
                                        ref={this.uploadInput}
                                        type="file"
                                        onChange={this.handleUpload}
                                    />
                                </React.Fragment>
                            ) : (
                                <button
                                    disabled={!project}
                                    title={t('newScene')}
                                    onClick={this.handleNewScene}
                                >{`+ ${t('newScene')}`}</button>
                            )}
                        </div>
                        {sidebarTab === 'files' ? this.renderFileTree() : this.renderScenes()}
                    </div>
                    <div className={styles.main}>
                        <div className={styles.tabs}>
                            {openTabs.map(tab => (
                                <div
                                    className={`${styles.tab} ${tab === activePath ? styles.activeTab : ''}`}
                                    key={tab}
                                    title={tab}
                                    onClick={() => this.openFile(tab)}
                                >
                                    <span>{basename(tab)}</span>
                                    {dirty.includes(tab) ? <span className={styles.dirty}>{'●'}</span> : null}
                                    <button
                                        className={styles.closeTab}
                                        onClick={event => this.closeTab(tab, event)}
                                    >{'×'}</button>
                                </div>
                            ))}
                        </div>
                        <div className={styles.editorArea}>
                            <div
                                className={styles.editor}
                                ref={this.editorHost}
                                style={{visibility: activePath && TEXT_EXTENSIONS.test(activePath) ? 'visible' : 'hidden'}}
                            />
                            {this.renderPreview()}
                            {activePath ? null : (
                                <div className={styles.placeholder}>{project ? t('noFile') : t('noProject')}</div>
                            )}
                        </div>
                    </div>
                </div>
                <div className={styles.bottom}>
                    <div className={styles.bottomTabs}>
                        <button
                            className={bottomTab === 'problems' ? styles.activeTab : ''}
                            onClick={() => this.setState({bottomTab: 'problems'})}
                        >{`${t('problems')} (${problems.length})`}</button>
                        <button
                            className={bottomTab === 'output' ? styles.activeTab : ''}
                            onClick={() => this.setState({bottomTab: 'output'})}
                        >{t('output')}</button>
                        <span className={styles.spacer} />
                        <span className={styles.lspStatus}>
                            {this.state.lspStatus === 'ready' ? t('lspReady') :
                                this.state.lspStatus === 'disconnected' ? t('lspDisconnected') : t('lspConnecting')}
                        </span>
                    </div>
                    <div className={styles.bottomBody}>
                        {bottomTab === 'problems' ? (
                            problems.length ? problems.map((problem, index) => (
                                <div
                                    className={styles.problem}
                                    key={index}
                                    onClick={() => this.revealProblem(problem)}
                                >
                                    <span className={styles.problemIcon}>{'⨯'}</span>
                                    <span className={styles.problemLocation}>
                                        {problem.file ? `${problem.file}:${problem.line}:${problem.column}` : ''}
                                    </span>
                                    <span>{problem.message}</span>
                                    <span className={styles.problemSource}>{problem.source}</span>
                                </div>
                            )) : <div className={styles.muted}>{t('noProblems')}</div>
                        ) : log.map((entry, index) => (
                            <div
                                className={entry.level === 'error' ? styles.logError : styles.logLine}
                                key={index}
                            >
                                <span className={styles.muted}>{entry.time}</span>{' '}{entry.message}
                            </div>
                        ))}
                    </div>
                </div>
            </div>
        );
    }
}

const fileIcon = filePath => {
    if (filePath.endsWith('.rks')) return '🎬';
    if (filePath.endsWith('.rk')) return '📜';
    if (IMAGE_EXTENSIONS.test(filePath)) return '🖼';
    if (AUDIO_EXTENSIONS.test(filePath)) return '🔊';
    return '📄';
};

RakeIDE.propTypes = {
    isDark: PropTypes.bool,
    isShowingProject: PropTypes.bool,
    locale: PropTypes.string,
    onSetProjectTitle: PropTypes.func,
    vm: PropTypes.instanceOf(VM).isRequired
};

const mapStateToProps = state => ({
    isDark: state.scratchGui.theme.theme.isDark(),
    isShowingProject: getIsShowingProject(state.scratchGui.projectState.loadingState),
    locale: state.locales.locale
});

const mapDispatchToProps = dispatch => ({
    onSetProjectTitle: title => dispatch(setProjectTitle(title))
});

export default connect(mapStateToProps, mapDispatchToProps)(RakeIDE);
