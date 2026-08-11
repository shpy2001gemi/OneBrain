/* Generated from onebrain-base-abi with pinned cbindgen; DO NOT EDIT. */


#ifndef ONEBRAIN_BASE_V1_H
#define ONEBRAIN_BASE_V1_H

#pragma once

#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

#define OB_BASE_ABI_MAJOR_V1 1

#define OB_BASE_ABI_MINOR_V1 0

#define OB_BASE_OK_V1 0

/**
 * Opaque scoped-management handle. Values are tokens and are never dereferenced.
 */
typedef struct ObBaseManagementV1 ObBaseManagementV1;

/**
 * Opaque ordinary Base handle. Values are tokens and are never dereferenced.
 */
typedef struct ObBaseRuntimeV1 ObBaseRuntimeV1;

typedef struct ObBaseCallV1 {
  uint32_t struct_size;
  uint16_t abi_major;
  uint16_t abi_minor;
  uint8_t process_generation[32];
  uint8_t dataset_generation[32];
  uint8_t request_id[32];
  uint8_t operation_id[32];
  uint8_t auxiliary_id[32];
  uint16_t discriminator;
  uint16_t flags;
  uint64_t value0;
  uint64_t value1;
  const uint8_t *payload_ptr;
  size_t payload_len;
} ObBaseCallV1;

typedef struct ObBaseOutputV1 {
  uint32_t struct_size;
  uint16_t abi_major;
  uint16_t abi_minor;
  uint8_t process_generation[32];
  uint8_t dataset_generation[32];
  uint16_t response_discriminator;
  uint16_t reserved;
  uint8_t operation_id[32];
  uint8_t *buffer_ptr;
  size_t buffer_capacity;
  size_t required_len;
  size_t written_len;
} ObBaseOutputV1;

typedef struct ObBaseErrorV1 {
  uint32_t struct_size;
  uint16_t abi_major;
  uint16_t abi_minor;
  uint16_t code;
  uint8_t retryable;
  uint8_t reconcile_before_retry;
  uint16_t reserved;
  const uint8_t *message_ptr;
  size_t message_len;
  uint64_t allocation_tag;
} ObBaseErrorV1;

typedef struct ObBaseOwnedBufferV1 {
  uint32_t struct_size;
  uint16_t abi_major;
  uint16_t abi_minor;
  const uint8_t *ptr;
  size_t len;
  uint64_t allocation_tag;
} ObBaseOwnedBufferV1;

typedef struct ObBaseOpenRequestV1 {
  uint32_t struct_size;
  uint16_t abi_major;
  uint16_t abi_minor;
  uint8_t registration_token[32];
  uint8_t host_trust_digest[32];
} ObBaseOpenRequestV1;

/**
 * SHA-256 of the canonical field-width/bound/ownership/discriminator and
 * lifecycle descriptor derived from the frozen Base v1 machine IDL.
 */
#define OB_BASE_IDL_DESCRIPTOR_SHA256_V1 { 15, 176, 51, 16, 169, 109, 146, 96, 101, 2, 103, 68, 94, 32, 211, 87, 198, 247, 231, 130, 193, 55, 90, 141, 123, 216, 168, 125, 153, 164, 40, 96, }

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

extern uint16_t ob_base_archive_capability_abort_v1(struct ObBaseManagementV1 *handle,
                                                    const struct ObBaseCallV1 *input,
                                                    struct ObBaseOutputV1 *output,
                                                    struct ObBaseErrorV1 *error);

extern uint16_t ob_base_archive_capability_destroy_v1(struct ObBaseManagementV1 *handle,
                                                      const struct ObBaseCallV1 *input,
                                                      struct ObBaseOutputV1 *output,
                                                      struct ObBaseErrorV1 *error);

extern uint16_t ob_base_archive_secret_register_v1(struct ObBaseManagementV1 *handle,
                                                   const struct ObBaseCallV1 *input,
                                                   struct ObBaseOutputV1 *output,
                                                   struct ObBaseErrorV1 *error);

extern uint16_t ob_base_archive_sink_begin_v1(struct ObBaseManagementV1 *handle,
                                              const struct ObBaseCallV1 *input,
                                              struct ObBaseOutputV1 *output,
                                              struct ObBaseErrorV1 *error);

extern uint16_t ob_base_archive_sink_commit_v1(struct ObBaseManagementV1 *handle,
                                               const struct ObBaseCallV1 *input,
                                               struct ObBaseOutputV1 *output,
                                               struct ObBaseErrorV1 *error);

extern uint16_t ob_base_archive_sink_read_chunk_v1(struct ObBaseManagementV1 *handle,
                                                   const struct ObBaseCallV1 *input,
                                                   struct ObBaseOutputV1 *output,
                                                   struct ObBaseErrorV1 *error);

extern uint16_t ob_base_archive_sink_read_v1(struct ObBaseManagementV1 *handle,
                                             const struct ObBaseCallV1 *input,
                                             struct ObBaseOutputV1 *output,
                                             struct ObBaseErrorV1 *error);

extern uint16_t ob_base_archive_source_begin_v1(struct ObBaseManagementV1 *handle,
                                                const struct ObBaseCallV1 *input,
                                                struct ObBaseOutputV1 *output,
                                                struct ObBaseErrorV1 *error);

extern uint16_t ob_base_archive_source_push_chunk_v1(struct ObBaseManagementV1 *handle,
                                                     const struct ObBaseCallV1 *input,
                                                     struct ObBaseOutputV1 *output,
                                                     struct ObBaseErrorV1 *error);

extern uint16_t ob_base_archive_source_push_v1(struct ObBaseManagementV1 *handle,
                                               const struct ObBaseCallV1 *input,
                                               struct ObBaseOutputV1 *output,
                                               struct ObBaseErrorV1 *error);

extern uint16_t ob_base_archive_source_seal_v1(struct ObBaseManagementV1 *handle,
                                               const struct ObBaseCallV1 *input,
                                               struct ObBaseOutputV1 *output,
                                               struct ObBaseErrorV1 *error);

/**
 * Release one tagged library-owned event/error allocation exactly once.
 *
 * # Safety
 * `buffer` and `error` must reference valid caller-owned public structures;
 * the buffer's pointer, length, and tag must be the unchanged binding
 * returned by this library.
 */
uint16_t ob_base_buffer_free_v1(struct ObBaseOwnedBufferV1 *buffer,
                                struct ObBaseErrorV1 *error);

extern uint16_t ob_base_cancel_v1(struct ObBaseRuntimeV1 *handle,
                                  const struct ObBaseCallV1 *input,
                                  struct ObBaseOutputV1 *output,
                                  struct ObBaseErrorV1 *error);

extern uint16_t ob_base_capabilities_v1(struct ObBaseRuntimeV1 *handle,
                                        const struct ObBaseCallV1 *input,
                                        struct ObBaseOutputV1 *output,
                                        struct ObBaseErrorV1 *error);

extern uint16_t ob_base_close_subscription_v1(struct ObBaseRuntimeV1 *handle,
                                              const struct ObBaseCallV1 *input,
                                              struct ObBaseOutputV1 *output,
                                              struct ObBaseErrorV1 *error);

extern uint16_t ob_base_close_v1(struct ObBaseRuntimeV1 *handle,
                                 const struct ObBaseCallV1 *input,
                                 struct ObBaseOutputV1 *output,
                                 struct ObBaseErrorV1 *error);

extern uint16_t ob_base_complete_reprovision_v1(struct ObBaseManagementV1 *handle,
                                                const struct ObBaseCallV1 *input,
                                                struct ObBaseOutputV1 *output,
                                                struct ObBaseErrorV1 *error);

extern uint16_t ob_base_complete_signer_reprovision_v1(struct ObBaseManagementV1 *handle,
                                                       const struct ObBaseCallV1 *input,
                                                       struct ObBaseOutputV1 *output,
                                                       struct ObBaseErrorV1 *error);

extern uint16_t ob_base_confirm_v1(struct ObBaseRuntimeV1 *handle,
                                   const struct ObBaseCallV1 *input,
                                   struct ObBaseOutputV1 *output,
                                   struct ObBaseErrorV1 *error);

extern uint16_t ob_base_drain_v1(struct ObBaseRuntimeV1 *handle,
                                 const struct ObBaseCallV1 *input,
                                 struct ObBaseOutputV1 *output,
                                 struct ObBaseErrorV1 *error);

/**
 * Revoke a scoped management handle and every capability it still owns.
 *
 * # Safety
 * `handle` must be a token returned by `ob_base_management_open_v1`; input,
 * output, and error pointers must reference valid caller-owned public
 * structures for the complete call.
 */
uint16_t ob_base_management_close_v1(struct ObBaseManagementV1 *handle,
                                     const struct ObBaseCallV1 *input,
                                     struct ObBaseOutputV1 *output,
                                     struct ObBaseErrorV1 *error);

/**
 * Consume one registered host grant into a scoped management handle.
 *
 * # Safety
 * `runtime` must be a live token returned by `ob_base_open_v1`; all other
 * pointers must remain valid for the call, and the input payload must cover
 * the exact registered 32-byte grant envelope.
 */
uint16_t ob_base_management_open_v1(struct ObBaseRuntimeV1 *runtime,
                                    const struct ObBaseCallV1 *input,
                                    struct ObBaseManagementV1 **out_handle,
                                    struct ObBaseOutputV1 *output,
                                    struct ObBaseErrorV1 *error);

extern uint16_t ob_base_negotiate_v1(struct ObBaseRuntimeV1 *handle,
                                     const struct ObBaseCallV1 *input,
                                     struct ObBaseOutputV1 *output,
                                     struct ObBaseErrorV1 *error);

/**
 * Open one host-registered Base service through an opaque C token.
 *
 * # Safety
 * `request`, `out_handle`, `output`, and `error` must point to caller-owned
 * storage that is valid for the full call and satisfies each public struct's
 * advertised `struct_size`. Any non-null buffer pointer must cover its stated
 * length/capacity.
 */
uint16_t ob_base_open_v1(const struct ObBaseOpenRequestV1 *request,
                         struct ObBaseRuntimeV1 **out_handle,
                         struct ObBaseOutputV1 *output,
                         struct ObBaseErrorV1 *error);

extern uint16_t ob_base_poll_events_v1(struct ObBaseRuntimeV1 *handle,
                                       const struct ObBaseCallV1 *input,
                                       struct ObBaseOutputV1 *output,
                                       struct ObBaseErrorV1 *error);

extern uint16_t ob_base_prepare_v1(struct ObBaseRuntimeV1 *handle,
                                   const struct ObBaseCallV1 *input,
                                   struct ObBaseOutputV1 *output,
                                   struct ObBaseErrorV1 *error);

extern uint16_t ob_base_query_v1(struct ObBaseRuntimeV1 *handle,
                                 const struct ObBaseCallV1 *input,
                                 struct ObBaseOutputV1 *output,
                                 struct ObBaseErrorV1 *error);

extern uint16_t ob_base_reconcile_v1(struct ObBaseRuntimeV1 *handle,
                                     const struct ObBaseCallV1 *input,
                                     struct ObBaseOutputV1 *output,
                                     struct ObBaseErrorV1 *error);

extern uint16_t ob_base_reserve_operation_v1(struct ObBaseRuntimeV1 *handle,
                                             const struct ObBaseCallV1 *input,
                                             struct ObBaseOutputV1 *output,
                                             struct ObBaseErrorV1 *error);

extern uint16_t ob_base_snapshot_v1(struct ObBaseRuntimeV1 *handle,
                                    const struct ObBaseCallV1 *input,
                                    struct ObBaseOutputV1 *output,
                                    struct ObBaseErrorV1 *error);

extern uint16_t ob_base_status_v1(struct ObBaseRuntimeV1 *handle,
                                  const struct ObBaseCallV1 *input,
                                  struct ObBaseOutputV1 *output,
                                  struct ObBaseErrorV1 *error);

extern uint16_t ob_base_subscribe_v1(struct ObBaseRuntimeV1 *handle,
                                     const struct ObBaseCallV1 *input,
                                     struct ObBaseOutputV1 *output,
                                     struct ObBaseErrorV1 *error);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* ONEBRAIN_BASE_V1_H */
