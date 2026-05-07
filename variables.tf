variable "project_id" {
  type        = string
  default     = "project-ec21338a-215e-427c-995"
  description = "GCP project ID"
}

variable "zone" {
  type        = string
  default     = "asia-south1-a"
  description = "GCP zone — single zone keeps Free Tier credit"
}

variable "cluster_name" {
  type        = string
  default     = "parcel-tracking-cluster"
}
