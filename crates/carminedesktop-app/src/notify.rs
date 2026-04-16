use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

pub fn mount_failed(app: &AppHandle, name: &str, reason: &str) {
    send(app, "Échec du montage", &format!("{name} : {reason}"));
}

pub fn mount_success(app: &AppHandle, name: &str, path: &str) {
    send(
        app,
        "Montage prêt",
        &format!("{name} est disponible dans {path}"),
    );
}

pub fn mounts_summary(app: &AppHandle, succeeded: usize, failed: usize) {
    let body = match (succeeded, failed) {
        (s, 0) if s > 0 => format!("{s} lecteur{} monté{}", plural(s), plural(s)),
        (s, f) if s > 0 && f > 0 => {
            format!("{s} lecteur{} monté{}, {f} en échec", plural(s), plural(s))
        }
        (0, f) if f > 0 => format!("Échec du montage de {f} lecteur{}", plural(f)),
        _ => return,
    };
    send(app, "Montages prêts", &body);
}

pub fn mount_not_found(app: &AppHandle, name: &str) {
    send(
        app,
        "Montage supprimé",
        &format!("'{name}' n'est plus accessible et a été retiré de votre configuration"),
    );
}

pub fn mount_orphaned(app: &AppHandle, name: &str) {
    send(
        app,
        "Montage supprimé",
        &format!("'{name}' a été supprimé ou déplacé et a été retiré de votre configuration"),
    );
}

pub fn mount_access_denied(app: &AppHandle, name: &str) {
    send(
        app,
        "Montage ignoré",
        &format!("Aucun accès à '{name}' \u{2014} vérifiez vos permissions"),
    );
}

pub fn auto_start_failed(app: &AppHandle, reason: &str) {
    send(
        app,
        "Démarrage automatique",
        &format!("Échec de l'enregistrement du démarrage automatique : {reason}"),
    );
}

pub fn sign_out_failed(app: &AppHandle, reason: &str) {
    send(
        app,
        "Échec de la déconnexion",
        &format!("La déconnexion a rencontré une erreur : {reason}"),
    );
}

pub fn auth_expired(app: &AppHandle) {
    send(
        app,
        "Session expirée",
        "Session expirée. Ouvrez Carmine Desktop pour vous reconnecter.",
    );
}

pub fn update_ready(app: &AppHandle, version: &str) {
    let app_name = app_display_name(app);
    send(
        app,
        "Mise à jour disponible",
        &format!("{app_name} v{version} est prêt \u{2014} redémarrez pour mettre à jour"),
    );
}

pub fn up_to_date(app: &AppHandle) {
    let app_name = app_display_name(app);
    send(app, "À jour", &format!("{app_name} est à jour"));
}

pub fn update_check_failed(app: &AppHandle) {
    send(
        app,
        "Échec de la vérification",
        "Impossible de vérifier les mises à jour. Réessayez plus tard.",
    );
}

pub fn update_not_configured(app: &AppHandle) {
    send(
        app,
        "Mises à jour",
        "La vérification des mises à jour n'est pas configurée pour cette version",
    );
}

pub fn conflict_detected(app: &AppHandle, file_name: &str, conflict_name: &str) {
    send(
        app,
        "Conflit de synchronisation",
        &format!(
            "'{file_name}' a été modifié sur un autre appareil. Votre version a été sauvegardée sous '{conflict_name}'."
        ),
    );
}

pub fn writeback_failed(app: &AppHandle, file_name: &str) {
    send(
        app,
        "Échec de l'enregistrement",
        &format!(
            "Échec de la sauvegarde des modifications de '{file_name}'. Vos modifications peuvent être perdues."
        ),
    );
}

pub fn upload_failed(app: &AppHandle, file_name: &str, reason: &str) {
    send(
        app,
        "Échec du téléversement",
        &format!("Échec du téléversement de '{file_name}' : {reason}"),
    );
}

pub fn delete_failed(app: &AppHandle, file_name: &str, reason: &str) {
    send(
        app,
        "Échec de la suppression",
        &format!("Échec de la suppression de '{file_name}' : {reason}"),
    );
}

pub fn file_locked(app: &AppHandle, file_name: &str) {
    send(
        app,
        "Fichier verrouillé",
        &format!(
            "'{file_name}' est en cours d'édition en ligne. Les modifications locales seront sauvegardées dans une copie séparée."
        ),
    );
}

pub fn deep_link_failed(app: &AppHandle, reason: &str) {
    send(
        app,
        "Ouvrir dans SharePoint",
        &format!("Impossible d'ouvrir le fichier : {reason}"),
    );
}

pub fn files_recovered(app: &AppHandle, count: usize, path: &str) {
    send(
        app,
        "Fichiers récupérés",
        &format!(
            "{count} fichier(s) non enregistré(s) récupéré(s) dans {path}. Ces fichiers n'ont pas été téléversés avant le dernier arrêt."
        ),
    );
}

pub fn offline_pin_started(app: &AppHandle, folder_name: &str) {
    send(
        app,
        "Téléchargement hors ligne",
        &format!(
            "Téléchargement de '{folder_name}' en cours \u{2014} le dossier sera bientôt disponible hors ligne. Suivez la progression dans Carmine Desktop \u{2192} Hors ligne."
        ),
    );
}

pub fn offline_pin_completed(app: &AppHandle, folder_name: &str) {
    send(
        app,
        "Disponible hors ligne",
        &format!("'{folder_name}' est maintenant disponible hors ligne."),
    );
}

pub fn offline_pin_rejected(app: &AppHandle, folder_name: &str, reason: &str) {
    send(
        app,
        "Hors ligne indisponible",
        &format!("Impossible de rendre '{folder_name}' disponible hors ligne : {reason}"),
    );
}

pub fn offline_pin_failed(app: &AppHandle, folder_name: &str, reason: &str) {
    send(
        app,
        "Erreur hors ligne",
        &format!(
            "Échec du téléchargement de '{folder_name}' pour une utilisation hors ligne : {reason}"
        ),
    );
}

pub fn offline_unpin_complete(app: &AppHandle, folder_name: &str) {
    send(
        app,
        "Espace libéré",
        &format!("'{folder_name}' n'est plus épinglé pour une utilisation hors ligne"),
    );
}

fn app_display_name(_app: &AppHandle) -> String {
    "Carmine Desktop".to_string()
}

fn plural(n: usize) -> &'static str {
    if n > 1 { "s" } else { "" }
}

pub(crate) fn send(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        tracing::warn!("failed to send notification '{title}': {e}");
    }
}
