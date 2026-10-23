export type PublicationSyncStatus = 'applied' | 'pending' | 'failed';

export interface PublicationSyncReport {
    status: PublicationSyncStatus;
    targets: Array<{
        machine_id: string;
        instance_id: string;
        status: PublicationSyncStatus;
        code: string;
    }>;
}

export interface PublicationMutationResult {
    publication_sync?: PublicationSyncReport;
}

/** A committed asset mutation is not proof that a public listener changed. */
export function publicationFeedback(
    result: unknown,
    locale: string,
    fallback: string,
    operation: 'save' | 'remove' | 'delete',
): { message: string; warning: boolean } {
    const value = result as PublicationMutationResult | undefined;
    const report = value?.publication_sync;
    if (!report || !Array.isArray(report.targets) || !report.targets.length) {
        return { message: fallback, warning: false };
    }
    const chinese = /^(cn|zh)(-|$)/i.test(locale);
    if (report.status === 'applied') {
        return {
            message: chinese
                ? operation === 'save'
                    ? '服务已保存，端口映射已下发并从运行网络回读确认。'
                    : operation === 'delete'
                        ? '服务已删除，相关端口映射已从运行网络清理并回读确认。'
                        : '已取消公开，相关端口映射已从运行网络清理并回读确认。'
                : operation === 'save'
                    ? 'Service saved. Port mappings were applied and verified on the running network.'
                    : operation === 'delete'
                        ? 'Service deleted. Its port mappings were removed and verified on the running network.'
                        : 'Publication disabled. Its port mappings were removed and verified on the running network.',
            warning: false,
        };
    }
    const reasons: Record<string, [string, string]> = {
        server_offline: ['公网服务器离线', 'Public server is offline'],
        session_changed: ['服务器连接发生变化', 'Server connection changed'],
        network_not_running: ['所选网络当前未运行', 'Selected network is not running'],
        runtime_unavailable: ['无法读取运行网络', 'Running network could not be read'],
        runtime_port_conflict: ['运行网络中的监听端口与已有映射冲突', 'A runtime listener conflicts with an existing mapping'],
        runtime_read_only: ['运行网络只读', 'Running network is read-only'],
        centrally_managed: ['网络已由中央控制器管理', 'Network is centrally managed'],
        instance_id_mismatch: ['网络实例身份校验失败', 'Network instance identity did not match'],
        saved_config_changed: ['网络配置已发生变化', 'Saved network configuration changed'],
        runtime_apply_failed: ['运行网络拒绝或未确认配置下发', 'Running network rejected or did not acknowledge the update'],
        runtime_readback_failed: ['配置下发后无法回读确认', 'Configuration readback failed after the update'],
        runtime_mapping_mismatch: ['回读的映射与保存内容不一致', 'Runtime mappings do not match the saved mappings'],
    };
    const details = report.targets.filter(target => target.status !== 'applied').map(target => {
        const reason = reasons[target.code]?.[chinese ? 0 : 1]
            ?? (chinese ? '映射同步尚未确认' : 'Mapping synchronization has not been confirmed');
        return `${reason} (${target.instance_id.slice(0, 8)})`;
    }).join(chinese ? '；' : '; ');
    const prefix = chinese
        ? operation === 'save'
            ? '服务资料已保存，但端口映射尚未确认生效。'
            : operation === 'delete'
                ? '服务资料已删除，但运行网络的映射尚未确认清理，原公网入口可能仍可访问。'
                : '取消公开配置已保存，但运行网络的映射尚未确认清理，原公网入口可能仍可访问。'
        : operation === 'save'
            ? 'Service data was saved, but the port mappings have not been confirmed on the runtime.'
            : operation === 'delete'
                ? 'Service data was deleted, but runtime mapping removal is unconfirmed. Public access may remain available.'
                : 'Publication was disabled in saved configuration, but runtime removal is unconfirmed. Public access may remain available.';
    return { message: `${prefix} ${details}`, warning: true };
}
