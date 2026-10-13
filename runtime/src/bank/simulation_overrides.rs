//! 交易模拟自定义账户覆盖扩展 (Simulation Custom Account Overrides Extension)
//!
//! 独立文件承载：允许在不提交更改至工作银行的前提下，注入/覆盖特定账户状态。
//! 避免在 bank.rs 中修改已有代码，消除后续 Agave 代码合并冲突。

use super::*;
use log::debug;

impl Bank {
    /// 执行交易模拟并应用自定义账户覆盖（零修改提交至工作银行）
    pub fn simulate_transaction_unchecked_with_overrides(
        &self,
        transaction: &impl TransactionWithMeta,
        enable_cpi_recording: bool,
        custom_overrides: AccountOverrides,
    ) -> TransactionSimulationResult {
        let account_keys = transaction.account_keys();
        let number_of_accounts = account_keys.len();

        let mut account_overrides = AccountOverrides::default();
        let slot_history_id = sysvar::slot_history::id();
        if account_keys.iter().any(|pubkey| *pubkey == slot_history_id) {
            if let Some(slot_history) = self.get_account_with_fixed_root(&slot_history_id) {
                account_overrides.set_slot_history(Some(slot_history));
            }
        }

        // 合并用户自定义注入的覆盖账户
        for (pubkey, account) in custom_overrides.iter() {
            account_overrides.set_account(pubkey, Some(account.clone()));
        }

        let batch = self.prepare_unlocked_batch_from_single_tx(transaction);
        let mut timings = ExecuteTimings::default();

        let LoadAndExecuteTransactionsOutput {
            mut processing_results,
            balance_collector,
            ..
        } = self.load_and_execute_transactions(
            &batch,
            self.max_processing_age()
                .saturating_sub(MAX_TRANSACTION_FORWARDING_DELAY),
            &mut timings,
            &mut TransactionErrorMetrics::default(),
            TransactionProcessingConfig {
                account_overrides: Some(&account_overrides),
                log_messages_bytes_limit: None,
                limit_to_load_programs: true,
                recording_config: ExecutionRecordingConfig {
                    enable_cpi_recording,
                    enable_log_recording: true,
                    enable_return_data_recording: true,
                    enable_transaction_balance_recording: true,
                },
                drop_on_failure: false,
                all_or_nothing: false,
                strict_nonce_size_check: true,
                drop_noop_transactions: true,
                drop_bail_out_transactions: true,
            },
        );

        debug!("simulate_transaction_with_overrides: {timings:?}");

        let processing_result = processing_results
            .pop()
            .unwrap_or(Err(TransactionError::InvalidProgramForExecution));
        let (
            post_simulation_accounts,
            result,
            fee,
            logs,
            return_data,
            inner_instructions,
            units_consumed,
            loaded_accounts_data_size,
        ) = match processing_result {
            Ok(processed_tx) => {
                let executed_units = processed_tx.executed_units();
                let loaded_accounts_data_size = processed_tx.loaded_accounts_data_size();

                match processed_tx {
                    ProcessedTransaction::Executed(executed_tx) => {
                        let details = executed_tx.execution_details;
                        let post_simulation_accounts = executed_tx
                            .loaded_transaction
                            .accounts
                            .into_iter()
                            .take(number_of_accounts)
                            .collect::<Vec<_>>();
                        (
                            post_simulation_accounts,
                            details.status,
                            Some(executed_tx.loaded_transaction.fee_details.total_fee()),
                            details.log_messages,
                            details.return_data,
                            details.inner_instructions,
                            executed_units,
                            loaded_accounts_data_size,
                        )
                    }
                    ProcessedTransaction::FeesOnly(fees_only_tx) => (
                        vec![],
                        Err(fees_only_tx.load_error),
                        Some(fees_only_tx.fee_details.total_fee()),
                        None,
                        None,
                        None,
                        executed_units,
                        loaded_accounts_data_size,
                    ),
                    ProcessedTransaction::NoOp(no_op_tx) => (
                        vec![],
                        Err(no_op_tx.validation_error),
                        None,
                        None,
                        None,
                        None,
                        executed_units,
                        loaded_accounts_data_size,
                    ),
                }
            }
            Err(error) => (vec![], Err(error), None, None, None, None, 0, 0),
        };
        let logs = logs.unwrap_or_default();

        let (pre_balances, post_balances, pre_token_balances, post_token_balances) =
            match balance_collector {
                Some(balance_collector) => {
                    let (mut native_pre, mut native_post, mut token_pre, mut token_post) =
                        balance_collector.into_vecs();

                    (
                        native_pre.pop(),
                        native_post.pop(),
                        token_pre.pop(),
                        token_post.pop(),
                    )
                }
                None => (None, None, None, None),
            };

        TransactionSimulationResult {
            result,
            logs,
            post_simulation_accounts,
            units_consumed,
            loaded_accounts_data_size,
            return_data,
            inner_instructions,
            fee,
            pre_balances,
            post_balances,
            pre_token_balances,
            post_token_balances,
        }
    }
}
